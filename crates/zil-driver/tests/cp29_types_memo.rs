//! Real source controls for the bounded, raw scalar-body epoch.
use std::collections::BTreeMap;
use zil_base::{
    FileId, Fingerprint, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile,
};
use zil_driver::{CompilationSession, CompileOptions, ProjectSources, QueryDisposition, Stage};
use zil_ir::{CoreExprKind, CoreProgram, CoreStatementKind, PassingMode, Type};
use zil_types::{ProgramMemo, check_program, check_program_with_memo};

fn core(texts: &[&str]) -> CoreProgram {
    CoreProgram {
        root: ModuleId(0),
        source_fingerprint: Fingerprint::ZERO,
        modules: texts
            .iter()
            .enumerate()
            .map(|(i, text)| {
                let source = SourceUnit::new(
                    FileId(i as u32),
                    ModuleId(i as u32),
                    format!("m{i}.zil"),
                    *text,
                );
                let parsed = zil_syntax::parse(&source).into_result().unwrap();
                zil_expand::expand_program_module(&source, &parsed, vec![])
                    .unwrap()
                    .0
            })
            .collect(),
    }
}
fn input(value: &str) -> ProjectSources {
    let texts = [
        "import a\nimport b\nfn main()->i32=>a.get()+b.get()".to_string(),
        format!("pub fn get()->i32=>value()\nfn value()->i32=>{value}"),
        "pub fn get()->i32=>2".to_string(),
    ];
    ProjectSources {
        root: ModuleId(0),
        modules: texts
            .into_iter()
            .enumerate()
            .map(|(i, text)| {
                SourceUnit::new(
                    FileId(i as u32),
                    ModuleId(i as u32),
                    format!("m{i}.zil"),
                    text,
                )
            })
            .collect(),
        paths: BTreeMap::from([
            (vec!["a".into()], ModuleId(1)),
            (vec!["b".into()], ModuleId(2)),
        ]),
    }
}

#[test]
fn private_body_edit_reuses_other_modules_with_exact_typed_output() {
    let target = TargetProfile::host_llvm();
    let limits = Limits::default();
    let a = core(&["fn a()->i32=>1", "fn b()->i32=>2", "fn c()->i32=>3"]);
    let (_, memo) = check_program_with_memo(
        &a,
        &target,
        &limits,
        Fingerprint::ZERO,
        &ProgramMemo::default(),
    )
    .unwrap();
    assert_eq!(memo.stats().checked_functions, 3);
    let b = core(&["fn a()->i32=>777", "fn b()->i32=>2", "fn c()->i32=>3"]);
    let (actual, next) =
        check_program_with_memo(&b, &target, &limits, Fingerprint::ZERO, &memo).unwrap();
    assert_eq!(actual, check_program(&b, &target, &limits).unwrap());
    assert_eq!(next.stats().reused_functions, 2);
    assert_eq!(next.stats().checked_functions, 1);
    let (restored, restored_memo) =
        check_program_with_memo(&a, &target, &limits, Fingerprint::ZERO, &next).unwrap();
    assert_eq!(restored, check_program(&a, &target, &limits).unwrap());
    assert_eq!(restored_memo.stats().reused_functions, 2);
    assert_eq!(
        memo.stats().checked_functions,
        3,
        "previous Arc epoch stays immutable"
    );
}

#[test]
fn complete_signature_identity_visibility_target_context_and_positions_are_bound() {
    let target = TargetProfile::host_llvm();
    let limits = Limits::default();
    let a = core(&["pub fn a(x:i32)->i32=>x", "fn b()->i32=>2"]);
    let (_, memo) = check_program_with_memo(
        &a,
        &target,
        &limits,
        Fingerprint::ZERO,
        &ProgramMemo::default(),
    )
    .unwrap();
    let mut variants = Vec::new();
    variants.push(core(&["pub fn a(x:i64)->i64=>x", "fn b()->i32=>2"]));
    variants.push(core(&["fn a(x:i32)->i32=>x", "fn b()->i32=>2"]));
    variants.push(core(&[
        "pub fn a(x:i32)->i32=>x\nfn added()->i32=>7",
        "fn b()->i32=>2",
    ]));
    let mut order = a.clone();
    order.modules.reverse();
    variants.push(order);
    let mut root = a.clone();
    root.root = ModuleId(1);
    variants.push(root);
    for changed in variants {
        let (typed, updated) =
            check_program_with_memo(&changed, &target, &limits, Fingerprint::ZERO, &memo).unwrap();
        assert_eq!(typed, check_program(&changed, &target, &limits).unwrap());
        assert_eq!(updated.stats().reused_functions, 0);
    }
    for (target, context) in [
        (TargetProfile::windows_x64_llvm(), Fingerprint::ZERO),
        (
            target.clone(),
            Fingerprint::of(b"new explicit build configuration"),
        ),
    ] {
        let (typed, updated) =
            check_program_with_memo(&a, &target, &limits, context, &memo).unwrap();
        assert_eq!(typed, check_program(&a, &target, &limits).unwrap());
        assert_eq!(updated.stats().reused_functions, 0);
    }
    let mut changed = core(&["pub fn a(x:i32)->i32=>x", "\n\nfn b()->i32=>2"]);
    // Even raw callers forging the source fingerprint cannot hide source spans.
    changed.modules[1].core.source_fingerprint = a.modules[1].core.source_fingerprint;
    let (typed, updated) =
        check_program_with_memo(&changed, &target, &limits, Fingerprint::ZERO, &memo).unwrap();
    assert_eq!(typed, check_program(&changed, &target, &limits).unwrap());
    assert_eq!(updated.stats().reused_functions, 1);
    let mut forged = a.clone();
    forged.modules[0].core.functions[0].parameters[0].mode = PassingMode::Move;
    assert_eq!(
        check_program_with_memo(&forged, &target, &limits, Fingerprint::ZERO, &memo).map(|p| p.0),
        check_program(&forged, &target, &limits)
    );
}

#[test]
fn all_eleven_limits_are_in_the_epoch_identity() {
    let target = TargetProfile::host_llvm();
    let a = core(&["fn a()->i32=>1", "fn b()->i32=>2"]);
    let limits = Limits::default();
    let (_, memo) = check_program_with_memo(
        &a,
        &target,
        &limits,
        Fingerprint::ZERO,
        &ProgramMemo::default(),
    )
    .unwrap();
    for dimension in 0..11 {
        let mut changed = limits.clone();
        match dimension {
            0 => changed.max_source_bytes += 1,
            1 => changed.max_tokens += 1,
            2 => changed.max_nodes += 1,
            3 => changed.max_functions += 1,
            4 => changed.max_blocks += 1,
            5 => changed.max_instructions += 1,
            6 => changed.max_locals += 1,
            7 => changed.max_values += 1,
            8 => changed.max_analysis_steps += 1,
            9 => changed.max_ct_steps += 1,
            _ => changed.max_call_depth += 1,
        }
        let (typed, updated) =
            check_program_with_memo(&a, &target, &changed, Fingerprint::ZERO, &memo).unwrap();
        assert_eq!(typed, check_program(&a, &target, &changed).unwrap());
        assert_eq!(updated.stats().reused_functions, 0, "dimension {dimension}");
    }
}

#[test]
fn cross_function_quota_shortage_replays_the_original_first_error() {
    let target = TargetProfile::host_llvm();
    let a = core(&[
        "fn a()->i32=>1",
        "fn b()->i32 { let x=2; if true { return x; } else { return 3; } }",
    ]);
    let b = core(&[
        "fn a()->i32 { let x=7; let y=8; if true { return x+y; } else { return 9; } }",
        "fn b()->i32 { let x=2; if true { return x; } else { return 3; } }",
    ]);
    for dimension in 0..4 {
        let set = |limits: &mut Limits, n| match dimension {
            0 => limits.max_blocks = n,
            1 => limits.max_instructions = n,
            2 => limits.max_locals = n,
            _ => limits.max_values = n,
        };
        let limits = (1..256)
            .find_map(|n| {
                let mut limits = Limits::default();
                set(&mut limits, n);
                let late_failure = check_program(&b, &target, &limits)
                    .err()
                    .and_then(|e| e.first().and_then(|e| e.span))
                    .is_some_and(|span| span.file == FileId(1));
                (check_program(&a, &target, &limits).is_ok() && late_failure).then_some(limits)
            })
            .unwrap();
        let (_, memo) = check_program_with_memo(
            &a,
            &target,
            &limits,
            Fingerprint::ZERO,
            &ProgramMemo::default(),
        )
        .unwrap();
        assert!(memo.stats().eligible);
        let expected = check_program(&b, &target, &limits).unwrap_err();
        assert_eq!(expected[0].category, ResultCategory::ResourceLimit);
        assert_eq!(expected[0].span.unwrap().file, FileId(1));
        let actual = check_program_with_memo(&b, &target, &limits, Fingerprint::ZERO, &memo)
            .err()
            .unwrap();
        assert_eq!(actual, expected, "dimension {dimension}");
        assert_eq!(
            check_program_with_memo(&a, &target, &limits, Fingerprint::ZERO, &memo)
                .unwrap()
                .0,
            check_program(&a, &target, &limits).unwrap()
        );
    }
}

#[test]
fn raw_body_edit_with_identical_spans_and_forged_fingerprint_cannot_hit() {
    let target = TargetProfile::host_llvm();
    let limits = Limits::default();
    let a = core(&["fn a()->i32=>1", "fn b()->i32=>2"]);
    let (_, memo) = check_program_with_memo(
        &a,
        &target,
        &limits,
        Fingerprint::ZERO,
        &ProgramMemo::default(),
    )
    .unwrap();
    let mut b = a.clone();
    let CoreStatementKind::Return(Some(expression)) =
        &mut b.modules[0].core.functions[0].body.statements[0].kind
    else {
        panic!("expanded expression body is a return");
    };
    expression.kind = CoreExprKind::Integer("9".into());
    assert_eq!(
        a.modules[0].core.source_fingerprint,
        b.modules[0].core.source_fingerprint
    );
    assert_eq!(
        a.modules[0].core.functions[0].span,
        b.modules[0].core.functions[0].span
    );
    let (actual, next) =
        check_program_with_memo(&b, &target, &limits, Fingerprint::ZERO, &memo).unwrap();
    assert_eq!(actual, check_program(&b, &target, &limits).unwrap());
    assert_ne!(actual, check_program(&a, &target, &limits).unwrap());
    assert_eq!(next.stats().reused_functions, 1);
    assert_eq!(next.stats().checked_functions, 1);
}

const SCALAR_IO: &str = r#"extern "C" fn getchar()->i32;
extern "C" fn putchar(n:i32)->i32;
pub fn get()->i32 {var n=0;unsafe{n=getchar()}return n}
pub fn put(n:i32){unsafe{putchar(n)}}"#;

#[test]
fn scalar_foreign_modules_run_live_and_full_foreign_contracts_invalidate() {
    let target = TargetProfile::host_llvm();
    let limits = Limits::default();
    let a = core(&[SCALAR_IO, "fn b()->i32=>2"]);
    let (_, memo) = check_program_with_memo(
        &a,
        &target,
        &limits,
        Fingerprint::ZERO,
        &ProgramMemo::default(),
    )
    .unwrap();
    assert!(memo.stats().eligible);
    assert_eq!(memo.stats().retained_modules, 1);
    let changed_io = SCALAR_IO.replace("var n=0", "var n=1");
    let b = core(&[&changed_io, "fn b()->i32=>2"]);
    let (actual, next) =
        check_program_with_memo(&b, &target, &limits, Fingerprint::ZERO, &memo).unwrap();
    assert_eq!(actual, check_program(&b, &target, &limits).unwrap());
    assert_eq!(next.stats().reused_functions, 1);
    assert_eq!(next.stats().checked_functions, 4);

    // These mutate raw declarations without moving any source identity/span.
    // Invalid contracts keep the old checker error; valid changes miss the epoch.
    for dimension in 0..4 {
        let mut changed = a.clone();
        let foreign = changed.modules[0].core.functions[0]
            .foreign
            .as_mut()
            .unwrap();
        match dimension {
            0 => foreign.symbol = "different_external_symbol".into(),
            1 => foreign.static_callbacks = !foreign.static_callbacks,
            2 => foreign.abi = "unsupported_abi".into(),
            _ => foreign.noescape_parameters = vec![0],
        }
        let expected = check_program(&changed, &target, &limits);
        let actual = check_program_with_memo(&changed, &target, &limits, Fingerprint::ZERO, &memo);
        if let Ok((_, next)) = &actual {
            assert_eq!(
                next.stats().reused_functions,
                0,
                "foreign field {dimension}"
            );
        }
        assert_eq!(actual.map(|p| p.0), expected, "foreign field {dimension}");
    }

    let unsafe_removed = SCALAR_IO.replace("unsafe{n=getchar()}", "n=getchar();");
    let invalid = core(&[&unsafe_removed, "fn b()->i32=>2"]);
    let expected = check_program(&invalid, &target, &limits).unwrap_err();
    assert_eq!(expected[0].code, "E-FFI-UNSAFE");
    assert_eq!(
        check_program_with_memo(&invalid, &target, &limits, Fingerprint::ZERO, &memo)
            .err()
            .unwrap(),
        expected
    );
    let handle = core(&[r#"extern "C" fn handle()->c_handle;"#, "fn b()->i32=>2"]);
    let (actual, next) =
        check_program_with_memo(&handle, &target, &limits, Fingerprint::ZERO, &memo).unwrap();
    assert_eq!(actual, check_program(&handle, &target, &limits).unwrap());
    assert!(!next.stats().eligible);
}

#[test]
fn persistent_project_with_scalar_io_still_verifies_private_edits() {
    let project = |value: &str| {
        let mut project = input(value);
        project.modules[0].text = "import a\nimport b\nimport io\nfn main()->i32{let x=a.get()+b.get();io.put(x);return x}".into();
        project
            .modules
            .push(SourceUnit::new(FileId(3), ModuleId(3), "io.zil", SCALAR_IO));
        project.paths.insert(vec!["io".into()], ModuleId(3));
        project
    };
    let options = CompileOptions::default();
    let mut session = CompilationSession::default();
    let first = session.compile_project(&project("1"), &options);
    assert_eq!(first.status, ResultCategory::Success, "{}", first.to_json());
    let changed = project("2");
    let cold = zil_driver::compile_project(&changed, &options);
    let hot = session.compile_project(&changed, &options);
    assert_eq!(hot.status, ResultCategory::Success, "{}", hot.to_json());
    assert_eq!(hot.diagnostics, cold.diagnostics);
    assert_eq!(hot.scalar_interfaces, cold.scalar_interfaces);
    assert_eq!(hot.verification_runs, 1);
    assert_eq!(session.type_memo_stats().reused_functions, 2);
    assert_eq!(session.type_memo_stats().checked_functions, 6);
    assert_eq!(session.type_memo_stats().retained_modules, 3);
}

#[test]
fn explicit_disable_drops_epoch_and_preserves_one_shot_results() {
    let options = CompileOptions::default();
    let mut session = CompilationSession::without_type_memo();
    for value in ["1", "2"] {
        let source = input(value);
        let actual = session.compile_project(&source, &options);
        let expected = zil_driver::compile_project(&source, &options);
        assert_eq!(actual.status, expected.status);
        assert_eq!(actual.scalar_interfaces, expected.scalar_interfaces);
        assert_eq!(
            session.type_memo_stats(),
            zil_types::ProgramMemoStats::default()
        );
        assert_eq!(session.retained_type_memo_stats().retained_modules, 0);
    }
    session.set_type_memo_enabled(true);
    assert_eq!(
        session.compile_project(&input("3"), &options).status,
        ResultCategory::Success
    );
    assert_eq!(session.type_memo_stats().retained_modules, 3);
    session.set_type_memo_enabled(false);
    assert_eq!(session.retained_type_memo_stats().retained_modules, 0);
    assert_eq!(
        session.compile_project(&input("4"), &options).status,
        ResultCategory::Success
    );
    assert!(!session.type_memo_stats().eligible);
}

#[test]
fn non_scalar_body_and_declarations_take_the_original_path() {
    let target = TargetProfile::host_llvm();
    let limits = Limits::default();
    for source in [
        "fn a()->i32 { let f=(x:i32)=>x+1; return f(2); }",
        "struct R { x:i32 }\nfn a()->i32 { let r=R {x:3}; return r.x; }",
        "fn a()->i32 { let a=[1,2]; return a[0]; }",
    ] {
        let source = core(&[source]);
        let expected = check_program(&source, &target, &limits);
        let actual = check_program_with_memo(
            &source,
            &target,
            &limits,
            Fingerprint::ZERO,
            &ProgramMemo::default(),
        );
        if let Ok((_, memo)) = &actual {
            assert!(!memo.stats().eligible);
        }
        assert_eq!(actual.map(|p| p.0), expected);
    }
    let mut bad = core(&["fn a()->i32=>1"]);
    bad.modules[0].core.schema_version += 1;
    assert_eq!(
        check_program_with_memo(
            &bad,
            &target,
            &limits,
            Fingerprint::ZERO,
            &ProgramMemo::default()
        )
        .map(|p| p.0),
        check_program(&bad, &target, &limits)
    );
    let mut recursive = core(&["fn a()->i32=>1"]);
    recursive.modules[0].core.functions[0].return_type = Type::Unsupported("test".into());
    assert_eq!(
        check_program_with_memo(
            &recursive,
            &target,
            &limits,
            Fingerprint::ZERO,
            &ProgramMemo::default()
        )
        .map(|p| p.0),
        check_program(&recursive, &target, &limits)
    );
}

#[test]
fn session_private_edit_runs_fresh_analysis_verify_and_clear_drops_epoch() {
    let mut session = CompilationSession::with_capacity(8);
    let options = CompileOptions::default();
    let first = session.compile_project(&input("1"), &options);
    assert_eq!(first.status, ResultCategory::Success, "{}", first.to_json());
    assert!(session.type_memo_stats().eligible);
    let changed = session.compile_project(&input("222"), &options);
    assert_eq!(
        changed.status,
        ResultCategory::Success,
        "{}",
        changed.to_json()
    );
    assert_eq!(session.type_memo_stats().reused_functions, 2);
    assert_eq!(session.type_memo_stats().checked_functions, 2);
    assert_eq!(changed.verification_runs, 1);
    assert!(
        changed
            .query_events
            .iter()
            .any(|event| event.phase == Phase::Analyze
                && event.disposition == QueryDisposition::Executed)
    );
    assert_eq!(
        changed.scalar_interfaces,
        zil_driver::compile_project(&input("222"), &options).scalar_interfaces
    );
    session.clear();
    assert_eq!(session.retained_type_memo_stats().retained_modules, 0);
    let after = session.compile_project(&input("333"), &options);
    assert_eq!(after.status, ResultCategory::Success, "{}", after.to_json());
    assert_eq!(session.type_memo_stats().reused_functions, 0);
}

#[test]
fn observer_cancellation_and_later_analysis_failure_do_not_commit_an_epoch() {
    let mut session = CompilationSession::with_capacity(8);
    let options = CompileOptions::default();
    assert_eq!(
        session.compile_project(&input("1"), &options).status,
        ResultCategory::Success
    );
    let retained = session.retained_type_memo_stats();
    let cancelled = CompileOptions::default();
    let result = session.compile_project_with_observer(&input("2"), &cancelled, &mut |event| {
        if event.phase == Phase::Types {
            cancelled.cancellation.cancel();
        }
    });
    assert_eq!(result.status, ResultCategory::Cancelled);
    assert_eq!(session.active_query_count(), 0);
    assert_eq!(session.retained_type_memo_stats(), retained);
    let mut uninitialized = input("1");
    uninitialized.modules[1].text =
        "pub fn get()->i32 { let mut x:i32; return x; }\nfn value()->i32=>1".into();
    let expected = zil_driver::compile_project(&uninitialized, &options);
    let actual = session.compile_project(&uninitialized, &options);
    assert_ne!(actual.status, ResultCategory::Success);
    assert_eq!(actual.diagnostics, expected.diagnostics);
    assert_eq!(session.retained_type_memo_stats(), retained);
    let retry = session.compile_project(&input("3"), &options);
    assert_eq!(retry.status, ResultCategory::Success, "{}", retry.to_json());
    assert_eq!(session.type_memo_stats().reused_functions, 2);
}

#[test]
fn raw_query_capacity_eviction_keeps_the_separate_bounded_successful_epoch() {
    let project = |value| ProjectSources {
        root: ModuleId(0),
        paths: BTreeMap::new(),
        modules: (0..40)
            .map(|i| {
                SourceUnit::new(
                    FileId(i),
                    ModuleId(i),
                    format!("m{i}.zil"),
                    if i == 0 {
                        format!("fn main()->i32=>{value}")
                    } else {
                        format!("fn f{i}()->i32=>{i}")
                    },
                )
            })
            .collect(),
    };
    let mut session = CompilationSession::default();
    let options = CompileOptions {
        stop_after: Stage::Verify,
        ..CompileOptions::default()
    };
    assert_eq!(
        session.compile_project(&project(1), &options).status,
        ResultCategory::Success
    );
    assert!(session.cached_query_count() > 32);
    let changed = session.compile_project(&project(2), &options);
    assert_eq!(
        changed.status,
        ResultCategory::Success,
        "{}",
        changed.to_json()
    );
    assert_eq!(changed.verification_runs, 1);
    assert_eq!(session.type_memo_stats().reused_functions, 39);
    assert_eq!(session.type_memo_stats().checked_functions, 1);
    assert_eq!(session.type_memo_stats().retained_modules, 40);
    assert!(session.type_memo_stats().retained_weight <= 64 * 1024 * 1024);
}
