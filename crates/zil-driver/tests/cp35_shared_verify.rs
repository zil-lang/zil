//! CP35: real source inputs, hostile raw neighbors, and fresh driver verification.
use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
use zil_base::{
    CompileResult, FileId, Fingerprint, Limits, ModuleId, Phase, ResultCategory, SourceUnit,
    TargetProfile,
};
use zil_driver::{
    CompilationSession, CompileOptions, CompileReport, NativeBackend, ProjectSources,
    QueryDisposition, Stage,
};
use zil_ir::{CandidatePlan, FunctionId, ScalarInterface, TypedModule, TypedProgram};
use zil_verify::{
    VerifiedModule, verify_module, verify_shared_module, verify_shared_program_module,
};

const SCALAR: &str = "fn main()->i32{let answer=40;return answer+2}";
const BORROW: &str = "fn id(x:&i32)->&i32=>x;fn main()->i32{let x=42;return *id(&x)}";

fn source(text: &str) -> SourceUnit {
    SourceUnit::new(FileId(0), ModuleId(0), "cp35.zil", text)
}
fn raw(text: &str) -> (TypedModule, CandidatePlan) {
    let source = source(text);
    let syntax = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand(&source, &syntax).unwrap();
    let module = zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default()).unwrap();
    let candidate = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    (module, candidate)
}
fn program(module: TypedModule) -> TypedProgram {
    TypedProgram {
        module,
        interfaces: Vec::new(),
        root_functions: Vec::new(),
    }
}
fn summary(
    result: CompileResult<VerifiedModule>,
) -> CompileResult<(TypedModule, CandidatePlan, Fingerprint)> {
    result.map(|checked| {
        (
            checked.module().clone(),
            checked.plan().clone(),
            checked.fingerprint(),
        )
    })
}
fn all_equal(
    module: &TypedModule,
    candidate: &CandidatePlan,
    target: &TargetProfile,
    limits: &Limits,
) {
    let owned = summary(verify_module(
        module.clone(),
        candidate.clone(),
        target,
        limits,
    ));
    let shared = summary(verify_shared_module(
        Arc::new(module.clone()),
        Arc::new(candidate.clone()),
        target,
        limits,
    ));
    let embedded = summary(verify_shared_program_module(
        Arc::new(program(module.clone())),
        Arc::new(candidate.clone()),
        target,
        limits,
    ));
    assert_eq!(shared, owned);
    assert_eq!(embedded, owned);
}

#[test]
fn owned_and_shared_real_source_results_and_lowering_are_equal() {
    for text in [
        SCALAR,
        BORROW,
        "fn main()->i32{let x=40;let add=(n:i32)=>x+n;return add(2)}",
        "fn main()->i32{let b=bytes_new();bytes_reserve(b,4);bytes_push(b,7u8);return i32(bytes_len(b))}",
    ] {
        let (module, candidate) = raw(text);
        let target = module.target.clone();
        let owned = verify_module(
            module.clone(),
            candidate.clone(),
            &target,
            &Limits::default(),
        )
        .unwrap();
        let module = Arc::new(module);
        let candidate = Arc::new(candidate);
        let shared = verify_shared_module(
            module.clone(),
            candidate.clone(),
            &target,
            &Limits::default(),
        )
        .unwrap();
        let program = Arc::new(program((*module).clone()));
        let embedded = verify_shared_program_module(
            program.clone(),
            candidate.clone(),
            &target,
            &Limits::default(),
        )
        .unwrap();
        for checked in [&shared, &embedded] {
            assert_eq!(checked.module(), owned.module());
            assert_eq!(checked.plan(), owned.plan());
            assert_eq!(checked.fingerprint(), owned.fingerprint());
            assert_eq!(format!("{checked:#?}"), format!("{owned:#?}"));
            assert_eq!(
                format!("{:?}", zil_lower::lower(checked).unwrap()),
                format!("{:?}", zil_lower::lower(&owned).unwrap())
            );
            for index in 0..module.functions.len() {
                let function = FunctionId(index as u32);
                assert_eq!(
                    checked.result_origins(function),
                    owned.result_origins(function)
                );
                assert_eq!(
                    checked.callable_contract(function),
                    owned.callable_contract(function)
                );
                assert_eq!(
                    checked.callable_return_contract(function),
                    owned.callable_return_contract(function)
                );
            }
        }
        assert!(std::ptr::eq(shared.module(), module.as_ref()));
        assert!(std::ptr::eq(embedded.module(), &program.module));
        assert!(std::ptr::eq(shared.plan(), candidate.as_ref()));
        assert!(std::ptr::eq(embedded.plan(), candidate.as_ref()));
        let clone = shared.clone();
        drop(shared);
        drop(module);
        drop(candidate);
        assert_eq!(clone.module(), owned.module());
        assert_eq!(clone.plan(), owned.plan());
    }
}

#[test]
fn module_and_plan_clone_on_write_cannot_change_a_checked_snapshot() {
    let (module, candidate) = raw(SCALAR);
    let target = module.target.clone();
    let mut module = Arc::new(module);
    let mut candidate = Arc::new(candidate);
    let checked = verify_shared_module(
        module.clone(),
        candidate.clone(),
        &target,
        &Limits::default(),
    )
    .unwrap();
    let old_module = checked.module().clone();
    let old_plan = checked.plan().clone();
    assert!(Arc::get_mut(&mut module).is_none());
    assert!(Arc::get_mut(&mut candidate).is_none());
    Arc::make_mut(&mut module).functions[0].id = FunctionId(9);
    Arc::make_mut(&mut candidate).module_fingerprint = module.fingerprint();
    assert_eq!(checked.module(), &old_module);
    assert_eq!(checked.plan(), &old_plan);
    let failure = verify_shared_module(
        module.clone(),
        candidate.clone(),
        &target,
        &Limits::default(),
    )
    .unwrap_err();
    assert_eq!(
        failure,
        verify_module(
            (*module).clone(),
            (*candidate).clone(),
            &target,
            &Limits::default()
        )
        .unwrap_err()
    );
    assert_eq!(Arc::strong_count(&module), 1);
    assert_eq!(Arc::strong_count(&candidate), 1);
    let good_module = Arc::new(old_module);
    Arc::make_mut(&mut candidate).module_fingerprint = Fingerprint::ZERO;
    assert!(verify_shared_module(good_module, candidate, &target, &Limits::default()).is_err());
    assert_eq!(checked.plan(), &old_plan);
}

#[test]
fn shared_program_preserves_aliases_without_granting_interface_authority() {
    let (module, candidate) = raw(BORROW);
    let target = module.target.clone();
    let owned = verify_module(
        module.clone(),
        candidate.clone(),
        &target,
        &Limits::default(),
    )
    .unwrap();
    let mut untrusted = program(module);
    untrusted.interfaces.push(ScalarInterface {
        module: ModuleId(999),
        exports: Vec::new(),
        callable_returns: Some(Vec::new()),
        signature_fingerprint: Fingerprint::of(b"forged interface"),
        implementation_fingerprint: Fingerprint::ZERO,
        result_origins: Some(Vec::new()),
        contract_fingerprint: Some(Fingerprint::ZERO),
        callable_contracts: Some(Vec::new()),
    });
    let mut program = Arc::new(untrusted);
    let candidate = Arc::new(candidate);
    let checked = verify_shared_program_module(
        program.clone(),
        candidate.clone(),
        &target,
        &Limits::default(),
    )
    .unwrap();
    assert!(std::ptr::eq(checked.module(), &program.module));
    assert!(Arc::get_mut(&mut program).is_none());
    assert_eq!(format!("{checked:#?}"), format!("{owned:#?}"));
    assert_eq!(checked.result_origins(FunctionId(0)), Some(vec![0]));
    assert_eq!(checked.callable_contract(FunctionId(0)), None);
    Arc::make_mut(&mut program).module.schema_version = 0;
    Arc::make_mut(&mut program).interfaces.clear();
    assert_eq!(checked.module(), owned.module());
    let failure = verify_shared_program_module(
        program.clone(),
        candidate.clone(),
        &target,
        &Limits::default(),
    )
    .unwrap_err();
    assert_eq!(
        failure,
        verify_module(
            program.module.clone(),
            (*candidate).clone(),
            &target,
            &Limits::default()
        )
        .unwrap_err()
    );
    assert_eq!(Arc::strong_count(&program), 1);
}

#[test]
fn repeated_shared_inputs_obey_current_limits_and_target() {
    // The address type exercises the checker's nonzero type-depth budget too.
    // Plain scalar types have depth zero and do not consume that quota.
    let (module, candidate) = raw(BORROW);
    let target = module.target.clone();
    let module = Arc::new(module);
    let candidate = Arc::new(candidate);
    let program = Arc::new(program((*module).clone()));
    let held = verify_shared_module(
        module.clone(),
        candidate.clone(),
        &target,
        &Limits::default(),
    )
    .unwrap();
    for field in 0..11 {
        let mut limits = Limits::default();
        match field {
            0 => limits.max_source_bytes = 0,
            1 => limits.max_tokens = 0,
            2 => limits.max_nodes = 0,
            3 => limits.max_functions = 0,
            4 => limits.max_blocks = 0,
            5 => limits.max_instructions = 0,
            6 => limits.max_locals = 0,
            7 => limits.max_values = 0,
            8 => limits.max_analysis_steps = 0,
            9 => limits.max_ct_steps = 0,
            10 => limits.max_call_depth = 0,
            _ => unreachable!(),
        }
        let expected = summary(verify_module(
            (*module).clone(),
            (*candidate).clone(),
            &target,
            &limits,
        ));
        assert_eq!(
            expected.is_err(),
            matches!(field, 3..=8 | 10),
            "the source must exercise each quota consumed by this checker; field {field}"
        );
        assert_eq!(
            summary(verify_shared_module(
                module.clone(),
                candidate.clone(),
                &target,
                &limits
            )),
            expected
        );
        assert_eq!(
            summary(verify_shared_program_module(
                program.clone(),
                candidate.clone(),
                &target,
                &limits
            )),
            expected
        );
    }
    let mut other_target = target.clone();
    other_target.id.push_str("-different-current-target");
    all_equal(&module, &candidate, &other_target, &Limits::default());
    assert!(
        verify_shared_module(
            module.clone(),
            candidate.clone(),
            &other_target,
            &Limits::default()
        )
        .is_err()
    );
    let again = verify_shared_module(module, candidate, &target, &Limits::default()).unwrap();
    assert_eq!(again.module(), held.module());
    assert_eq!(again.plan(), held.plan());
}

#[test]
fn malformed_raw_neighbors_keep_complete_owned_diagnostics() {
    let (module, candidate) = raw(SCALAR);
    for mutation in 0..6 {
        let mut module = module.clone();
        let mut candidate = candidate.clone();
        match mutation {
            0 => module.schema_version = 0,
            1 => candidate.schema_version = 0,
            2 => candidate.module_fingerprint = Fingerprint::ZERO,
            3 => candidate.functions.clear(),
            4 => {
                module.functions[0].id = FunctionId(9);
                candidate.module_fingerprint = module.fingerprint();
            }
            5 => {
                candidate.functions[0].blocks[0]
                    .initialized_in
                    .push(zil_ir::LocalId(0));
            }
            _ => unreachable!(),
        }
        let target = module.target.clone();
        assert!(
            verify_module(
                module.clone(),
                candidate.clone(),
                &target,
                &Limits::default()
            )
            .is_err()
        );
        all_equal(&module, &candidate, &target, &Limits::default());
    }
}

fn sources() -> ProjectSources {
    ProjectSources {
        root: ModuleId(0),
        modules: vec![
            source("import leaf;fn main()->i32=>leaf.answer()"),
            SourceUnit::new(
                FileId(1),
                ModuleId(1),
                "leaf.zil",
                "pub fn answer()->i32=>42",
            ),
        ],
        paths: BTreeMap::from([(vec!["leaf".into()], ModuleId(1))]),
    }
}
fn success(report: &CompileReport) {
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1, "{}", report.to_json());
    assert_eq!(
        report
            .query_events
            .iter()
            .filter(|event| event.phase == Phase::Verify
                && event.disposition == QueryDisposition::Executed)
            .count(),
        1
    );
}

#[test]
fn single_and_project_sessions_run_fresh_verify_after_reuse_and_cancellation() {
    for project in [false, true] {
        let mut session = CompilationSession::with_capacity(8192);
        let options = CompileOptions {
            stop_after: Stage::Lower,
            ..CompileOptions::default()
        };
        let run = |session: &mut CompilationSession,
                   options: &CompileOptions,
                   observer: &mut dyn FnMut(&zil_driver::QueryEvent)| {
            if project {
                session.compile_project_with_observer(&sources(), options, observer)
            } else {
                session.compile_source_with_observer(&source(SCALAR), options, observer)
            }
        };
        let cold = run(&mut session, &options, &mut |_| {});
        success(&cold);
        let warm = run(&mut session, &options, &mut |_| {});
        success(&warm);
        assert!(warm.cache_hit);
        assert!(
            warm.query_events
                .iter()
                .any(|event| event.phase == Phase::Analyze
                    && event.disposition == QueryDisposition::Reused)
        );
        assert_eq!(warm.scalar_interfaces, cold.scalar_interfaces);
        assert_eq!(warm.phase, Phase::Lower);
        for phase in [Phase::Analyze, Phase::Verify] {
            let cancelled = CompileOptions {
                stop_after: Stage::Lower,
                ..CompileOptions::default()
            };
            let token = cancelled.cancellation.clone();
            let report = run(&mut session, &cancelled, &mut |event| {
                if event.phase == phase {
                    token.cancel();
                }
            });
            assert_eq!(
                report.status,
                ResultCategory::Cancelled,
                "{}",
                report.to_json()
            );
            assert_eq!(
                report.verification_runs,
                usize::from(phase == Phase::Verify)
            );
            assert_eq!(session.active_query_count(), 0);
            let restored = run(&mut session, &options, &mut |_| {});
            success(&restored);
            assert_eq!(restored.scalar_interfaces, cold.scalar_interfaces);
        }
        let fresh = run(&mut CompilationSession::default(), &options, &mut |_| {});
        assert_eq!(fresh.scalar_interfaces, cold.scalar_interfaces);
        assert_eq!(fresh.diagnostics, cold.diagnostics);
        assert_eq!(fresh.input_fingerprint, cold.input_fingerprint);
    }
}

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Case(PathBuf);
impl Case {
    fn new() -> Self {
        let root = std::env::var_os("ZIL_TEST_REPORT_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join(format!(
                "cp35-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        fs::create_dir_all(&root).unwrap();
        Self(root)
    }
    fn report(&self, label: &str, report: &CompileReport) {
        fs::write(self.0.join(format!("{label}.json")), report.to_json()).unwrap();
    }
}

#[test]
fn filesystem_checkpoint_moves_raw_inputs_and_runs_fresh_verify_and_native() {
    let case = Case::new();
    fs::write(
        case.0.join("main.zil"),
        "import leaf;fn value()->i32=>leaf.answer()",
    )
    .unwrap();
    fs::write(case.0.join("leaf.zil"), "pub fn answer()->i32=>42").unwrap();
    let options = CompileOptions {
        stop_after: Stage::Build,
        backend: NativeBackend::Clang,
        entry: Some("value".into()),
        output: Some(case.0.join("native")),
        query_cache: Some(case.0.join("queries")),
        ..CompileOptions::default()
    };
    let mut prior = None;
    for (label, expected) in [("cold", "miss"), ("warm", "hit")] {
        let mut session = CompilationSession::default();
        let report = session.compile_path(&case.0.join("main.zil"), &options);
        case.report(label, &report);
        success(&report);
        assert_eq!(report.query_cache.state, expected, "{}", report.to_json());
        assert_eq!(session.active_query_count(), 0);
        let run = Command::new(options.output.as_ref().unwrap())
            .output()
            .unwrap();
        fs::write(case.0.join(format!("{label}.stdout")), &run.stdout).unwrap();
        fs::write(case.0.join(format!("{label}.stderr")), &run.stderr).unwrap();
        assert!(run.status.success());
        assert_eq!(run.stdout, b"42\n");
        assert!(run.stderr.is_empty());
        if let Some(interfaces) = prior {
            assert_eq!(report.scalar_interfaces, interfaces);
        }
        prior = Some(report.scalar_interfaces);
    }
    let before = fs::read(options.output.as_ref().unwrap()).unwrap();
    let cancelled = options.clone();
    cancelled.cancellation.cancel();
    let report = CompilationSession::default().compile_path(&case.0.join("main.zil"), &cancelled);
    case.report("cancelled", &report);
    assert_eq!(report.status, ResultCategory::Cancelled);
    assert_eq!(report.verification_runs, 0);
    assert_eq!(fs::read(options.output.as_ref().unwrap()).unwrap(), before);
}
