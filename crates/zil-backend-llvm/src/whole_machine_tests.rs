//! Complete emitter products remain raw values behind current Verify/LIR bind.
use super::*;

type MachineResult = zil_base::CompileResult<(zil_llvm_api::model::Module, bool, usize)>;

fn fixture(value: i64, target: TargetProfile, limits: &Limits) -> VerifiedModule {
    let first = returning(Constant::I64(value));
    let mut second = returning(Constant::I64(99));
    second.id = FunctionId(1);
    second.name = "sibling".into();
    let mut module = checked(vec![first, second], vec![vec![(vec![], vec![])]; 2])
        .module()
        .clone();
    module.target = target.clone();
    recheck(module, limits)
}
fn recheck(module: TypedModule, limits: &Limits) -> VerifiedModule {
    let target = module.target.clone();
    let mut plan = CandidatePlan::empty_for(&module);
    plan.functions = module
        .functions
        .iter()
        .map(|f| FunctionPlan {
            function: f.id,
            holds: vec![ValueHold {
                value: ValueId(0),
                roots: vec![],
            }]
            .into(),
            blocks: vec![BlockPlan {
                block: BlockId(0),
                initialized_in: vec![],
                initialized_out: vec![],
            }]
            .into(),
        })
        .collect();
    verify_module(module, plan, &target, limits).unwrap()
}
fn lower(
    verified: &VerifiedModule,
    raw: &LirModule,
    entry: EntryPoint,
    scalar: bool,
    closed: bool,
    jobs: usize,
    session: Option<&mut crate::emit::incremental::Session>,
) -> MachineResult {
    crate::emit::lower_view_with_session(
        verified,
        &raw.as_view(),
        &EmitOptions { entry },
        scalar,
        closed,
        jobs,
        session,
    )
}

#[test]
fn whole_machine_current_input_shares_output_and_tracks_edit_failure_recovery() {
    for (scalar, closed) in [(false, false), (true, false), (true, true)] {
        let mut session = crate::emit::incremental::Session::default();
        let mut previous = None;
        for (index, value) in [42, 42, 43, 43].into_iter().enumerate() {
            // Every request supplies a newly independently checked capability.
            let current = fixture(value, TargetProfile::host_llvm(), &Limits::default());
            let raw = lir(&current);
            let actual = lower(
                &current,
                &raw,
                EntryPoint::None,
                scalar,
                closed,
                1,
                Some(&mut session),
            )
            .unwrap();
            let expected =
                lower(&current, &raw, EntryPoint::None, scalar, closed, 1, None).unwrap();
            assert_eq!(actual, expected);
            assert_eq!(actual.0.to_text(), expected.0.to_text());
            zil_llvm_api::validate_module(&actual.0).unwrap();
            let whole = index == 1 || index == 3;
            assert_eq!(session.reused_machine_module, whole);
            if whole {
                assert_eq!(
                    (session.hits, session.misses),
                    (0, 0),
                    "whole-result use is not a function consumer hit"
                );
                assert_eq!(
                    session.whole_machine_input_functions_compared,
                    raw.functions.len()
                );
                assert!(actual.0.items.ptr_eq(previous.as_ref().unwrap()));
            } else {
                assert!(session.misses > 0);
            }
            previous = Some(actual.0.items);
        }
        let current = fixture(43, TargetProfile::host_llvm(), &Limits::default());
        let mut stale = lir(&current);
        stale.functions[0].blocks[0].instructions[0].kind =
            InstructionKind::Constant(Constant::I64(900));
        let failure = lower(
            &current,
            &stale,
            EntryPoint::None,
            scalar,
            closed,
            1,
            Some(&mut session),
        );
        assert_eq!(
            failure,
            lower(&current, &stale, EntryPoint::None, scalar, closed, 1, None)
        );
        assert_eq!(failure.unwrap_err()[0].code, "E-CODEGEN-UNBOUND-LIR");
        assert!(!session.reused_machine_module);
        let raw = lir(&current);
        let recovery = lower(
            &current,
            &raw,
            EntryPoint::None,
            scalar,
            closed,
            1,
            Some(&mut session),
        )
        .unwrap();
        assert_eq!(
            recovery,
            lower(&current, &raw, EntryPoint::None, scalar, closed, 1, None).unwrap()
        );
        assert!(session.reused_machine_module);
    }
}

#[test]
fn whole_machine_hit_cannot_authorize_any_substituted_public_lir_shape() {
    let current = fixture(42, TargetProfile::host_llvm(), &Limits::default());
    let original = lir(&current);
    let mut session = crate::emit::incremental::Session::default();
    lower(
        &current,
        &original,
        EntryPoint::None,
        false,
        false,
        1,
        Some(&mut session),
    )
    .unwrap();
    let mut mutants = Vec::new();
    macro_rules! mutant {
        ($body:expr) => {{
            let mut raw = original.clone();
            ($body)(&mut raw);
            mutants.push(raw);
        }};
    }
    mutant!(|r: &mut LirModule| r.schema_version += 1);
    mutant!(|r: &mut LirModule| r.module_id.0 += 1);
    mutant!(|r: &mut LirModule| r.verified_fingerprint = Fingerprint::ZERO);
    mutant!(|r: &mut LirModule| r.target.triple = "x86_64-pc-windows-msvc".into());
    mutant!(|r: &mut LirModule| r.layouts[0].size += 1);
    mutant!(|r: &mut LirModule| r.layouts[0].alignment *= 2);
    mutant!(|r: &mut LirModule| r.layouts.reverse());
    mutant!(|r: &mut LirModule| r.functions.clear());
    mutant!(|r: &mut LirModule| r.functions.reverse());
    mutant!(|r: &mut LirModule| r.functions[0].name.push_str("renamed"));
    mutant!(|r: &mut LirModule| r.functions[0].span = Span::new(zil_base::FileId(10), 20, 30));
    mutant!(|r: &mut LirModule| r.functions[0].parameters.push(LocalId(0)));
    mutant!(|r: &mut LirModule| r.functions[0].entry = BlockId(100));
    mutant!(|r: &mut LirModule| r.functions[0].call_kind = CallKind::Suspend);
    mutant!(|r: &mut LirModule| r.functions[0].environment = Some(ClosureId(0)));
    mutant!(|r: &mut LirModule| r.functions[0].return_type = Type::I32);
    mutant!(|r: &mut LirModule| r.functions[0].result_mode = PassingMode::Give);
    mutant!(|r: &mut LirModule| r.functions[0].reference_return_bound = Some(vec![0]));
    mutant!(
        |r: &mut LirModule| r.functions[0].storage_life = Some(StorageLifeContract {
            fixed_binder_count: 0,
            invocation_binder_count: 0,
            bindings: vec![],
            classes: vec![],
        })
    );
    mutant!(
        |r: &mut LirModule| r.functions[0].foreign = Some(ForeignDeclaration {
            abi: "C".into(),
            symbol: "foreign_replacement".into(),
            static_callbacks: false,
            noescape_parameters: vec![],
            scoped_callback: None,
        })
    );
    mutant!(|r: &mut LirModule| r.functions[0].locals.push(local(0, Type::I64, false)));
    mutant!(|r: &mut LirModule| r.functions[0].values[0] = Type::I32);
    mutant!(|r: &mut LirModule| r.functions[0].blocks[0].terminator = Terminator::Trap);
    mutant!(
        |r: &mut LirModule| r.functions[0].blocks[0].instructions[0].span =
            Span::new(zil_base::FileId(0), 2, 5)
    );
    mutant!(|r: &mut LirModule| r.resources.push(ResourceDefinition {
        name: "extra".into(),
        payload: Type::I32,
        teardown: None
    }));
    mutant!(|r: &mut LirModule| r.closures.push(ClosureDefinition {
        storage_life: None,
        id: ClosureId(0),
        function: FunctionId(1),
        captures: vec![],
        parameters: vec![],
        result: Type::I64,
        kind: CallableKind::Fn,
        lending_captures: vec![]
    }));
    for raw in mutants {
        let expected = lower(&current, &raw, EntryPoint::None, false, false, 1, None);
        let actual = lower(
            &current,
            &raw,
            EntryPoint::None,
            false,
            false,
            1,
            Some(&mut session),
        );
        assert_eq!(actual, expected);
        assert_eq!(actual.unwrap_err()[0].code, "E-CODEGEN-UNBOUND-LIR");
        assert!(!session.reused_machine_module);
        // Rejected substituted inputs never become successful cache products.
        assert!(
            lower(
                &current,
                &original,
                EntryPoint::None,
                false,
                false,
                1,
                Some(&mut session)
            )
            .is_ok()
        );
        assert!(session.reused_machine_module);
    }
}

#[test]
fn whole_machine_options_full_limits_target_and_spans_remain_current_inputs() {
    let host = TargetProfile::host_llvm();
    let mut session = crate::emit::incremental::Session::default();
    let current = fixture(42, host.clone(), &Limits::default());
    let raw = lir(&current);
    lower(
        &current,
        &raw,
        EntryPoint::None,
        false,
        false,
        1,
        Some(&mut session),
    )
    .unwrap();
    for (entry, scalar, closed, jobs) in [
        (EntryPoint::ReturnValue(FunctionId(0)), false, false, 1),
        (EntryPoint::ReturnValue(FunctionId(0)), true, false, 1),
        (EntryPoint::ReturnValue(FunctionId(0)), true, true, 1),
        (EntryPoint::ReturnValue(FunctionId(0)), true, true, 2),
    ] {
        let expected = lower(&current, &raw, entry, scalar, closed, jobs, None).unwrap();
        let actual = lower(
            &current,
            &raw,
            entry,
            scalar,
            closed,
            jobs,
            Some(&mut session),
        )
        .unwrap();
        assert_eq!(actual, expected);
        assert!(
            !session.reused_machine_module,
            "changed actual emitter option must fall back"
        );
        assert_eq!(
            lower(
                &current,
                &raw,
                entry,
                scalar,
                closed,
                jobs,
                Some(&mut session)
            )
            .unwrap(),
            expected
        );
        assert!(session.reused_machine_module);
    }
    let mut limits = Limits::default();
    // Only a non-ledger limit changes: prefix remaining and actual options stay
    // identical, so checking just remaining fuel would accept this incorrectly.
    limits.max_call_depth += 1;
    let changed_limits = fixture(42, host, &limits);
    let changed_raw = lir(&changed_limits);
    let entry = EntryPoint::ReturnValue(FunctionId(0));
    assert_eq!(
        lower(
            &changed_limits,
            &changed_raw,
            entry,
            true,
            true,
            2,
            Some(&mut session)
        ),
        lower(&changed_limits, &changed_raw, entry, true, true, 2, None)
    );
    assert!(!session.reused_machine_module);
    lower(
        &changed_limits,
        &changed_raw,
        entry,
        true,
        true,
        2,
        Some(&mut session),
    )
    .unwrap();
    assert!(session.reused_machine_module);
    let changed_target = fixture(42, TargetProfile::windows_x64_llvm(), &limits);
    let target_raw = lir(&changed_target);
    assert_eq!(
        lower(
            &changed_target,
            &target_raw,
            entry,
            true,
            true,
            2,
            Some(&mut session)
        ),
        lower(&changed_target, &target_raw, entry, true, true, 2, None)
    );
    assert!(!session.reused_machine_module);
    lower(
        &changed_target,
        &target_raw,
        entry,
        true,
        true,
        2,
        Some(&mut session),
    )
    .unwrap();
    assert!(session.reused_machine_module);
    let mut module = changed_target.module().clone();
    module.functions[0].span = Span::new(zil_base::FileId(0), 0, 50);
    module.functions[0].blocks[0].instructions[0].span = Span::new(zil_base::FileId(0), 1, 2);
    let moved_spans = recheck(module, &limits);
    let span_raw = lir(&moved_spans);
    assert_eq!(
        lower(
            &moved_spans,
            &span_raw,
            entry,
            true,
            true,
            2,
            Some(&mut session)
        ),
        lower(&moved_spans, &span_raw, entry, true, true, 2, None)
    );
    assert!(
        !session.reused_machine_module,
        "full actual input spans must not be silently projected out"
    );
    lower(
        &moved_spans,
        &span_raw,
        entry,
        true,
        true,
        2,
        Some(&mut session),
    )
    .unwrap();
    assert!(
        session.reused_machine_module,
        "current exact owner retained even when per-function native equality ignored spans"
    );
}

#[test]
fn whole_machine_exact_original_emitter_budget_n_and_n_minus_one_preserve_first_error() {
    let current = fixture(42, TargetProfile::host_llvm(), &Limits::default());
    let raw = lir(&current);
    let mut low = 0;
    let mut high = current.limits().max_analysis_steps;
    while high - low > 1 {
        let middle = low + (high - low) / 2;
        let result = crate::emit::incremental::with_initial_work(middle, || {
            lower(&current, &raw, EntryPoint::None, false, false, 1, None)
        });
        match result {
            Ok(_) => high = middle,
            Err(errors) => {
                assert_eq!(errors[0].category, ResultCategory::ResourceLimit);
                low = middle;
            }
        }
    }
    let n = high;
    assert!(n > 0);
    let mut session = crate::emit::incremental::Session::default();
    for (index, budget) in [n, n, n - 1, n, n].into_iter().enumerate() {
        let expected = crate::emit::incremental::with_initial_work(budget, || {
            lower(&current, &raw, EntryPoint::None, false, false, 1, None)
        });
        let actual = crate::emit::incremental::with_initial_work(budget, || {
            lower(
                &current,
                &raw,
                EntryPoint::None,
                false,
                false,
                1,
                Some(&mut session),
            )
        });
        assert_eq!(
            actual, expected,
            "complete machine, remaining ledger or original first error differs at {budget}"
        );
        // The N-1 failure cannot publish or evict the completed N epoch.
        // Returning to its exact starting ledger may therefore reuse it at
        // index 3 as well; the failed request itself must remain a miss.
        assert_eq!(session.reused_machine_module, budget == n && index > 0);
        if budget == n {
            assert_eq!(actual.unwrap().2, 0);
        } else {
            let error = actual.unwrap_err();
            assert_eq!(error[0].category, ResultCategory::ResourceLimit);
            assert!(error[0].message.contains("ABI") || error[0].message.contains("budget"));
        }
    }
}

#[cfg(all(feature = "llvm-api", target_os = "linux"))]
#[test]
fn whole_machine_native_cold_unchanged_edit_failure_recovery_match_fresh_llvm_and_elf() {
    let directory = TestDirectory::new();
    let mut options = BuildOptions::new(directory.0.join("program"));
    options.backend = NativeBackend::LlvmApi;
    options.optimization = Optimization::O2;
    options.codegen.units = 2;
    options.object_cache = Some(directory.0.join("objects"));
    options.emit.entry = EntryPoint::ReturnValue(FunctionId(0));
    let mut session = BackendSession::default();
    for (index, value) in [42, 42, 43, 43, 43].into_iter().enumerate() {
        let current = fixture(value, TargetProfile::host_llvm(), &Limits::default());
        let raw = lir(&current);
        if index == 4 {
            let mut stale = raw.clone();
            stale.layouts[0].size += 1;
            let failure = build_view_with_cancellation_and_session(
                &current,
                &stale.as_view(),
                &options,
                &|| false,
                None,
                Some(&mut session),
            )
            .unwrap_err();
            assert_eq!(failure[0].code, "E-CODEGEN-UNBOUND-LIR");
            assert!(!session.summary().reused_machine_module);
        }
        let mut actual = build_view_with_cancellation_and_session(
            &current,
            &raw.as_view(),
            &options,
            &|| false,
            None,
            Some(&mut session),
        )
        .unwrap();
        actual.commit_object_cache();
        assert_eq!(
            session.summary().reused_machine_module,
            index == 1 || index >= 3
        );
        if session.summary().reused_machine_module {
            assert_eq!(
                (
                    session.summary().lowered_functions,
                    session.summary().reused_functions
                ),
                (0, 0)
            );
        }
        let llvm = fs::read(&actual.llvm_ir).unwrap();
        let elf = fs::read(&actual.executable).unwrap();
        let run = Command::new(&actual.executable).output().unwrap();
        assert!(run.status.success());
        assert_eq!(run.stdout, format!("{value}\n").as_bytes());
        assert!(run.stderr.is_empty());
        let mut fresh_options = options.clone();
        fresh_options.object_cache = None;
        let expected = build_view_with_cancellation_and_session(
            &current,
            &raw.as_view(),
            &fresh_options,
            &|| false,
            None,
            None,
        )
        .unwrap();
        assert_eq!(llvm, fs::read(&expected.llvm_ir).unwrap());
        assert_eq!(elf, fs::read(&expected.executable).unwrap());
        if index == 2 {
            assert!(
                actual
                    .codegen
                    .object_reports
                    .iter()
                    .any(|report| report.contains("\"verify_before_after\":true"))
            );
        }
    }
}

#[test]
fn whole_machine_post_binding_option_error_preserves_only_completed_history() {
    let current = fixture(42, TargetProfile::host_llvm(), &Limits::default());
    let raw = lir(&current);
    let mut session = crate::emit::incremental::Session::default();
    lower(
        &current,
        &raw,
        EntryPoint::None,
        false,
        false,
        1,
        Some(&mut session),
    )
    .unwrap();
    let bad_entry = EntryPoint::ReturnValue(FunctionId(900));
    let actual = lower(
        &current,
        &raw,
        bad_entry,
        false,
        false,
        1,
        Some(&mut session),
    );
    let expected = lower(&current, &raw, bad_entry, false, false, 1, None);
    assert!(actual.is_err());
    assert_eq!(actual, expected);
    assert!(!session.reused_machine_module);
    let restored = lower(
        &current,
        &raw,
        EntryPoint::None,
        false,
        false,
        1,
        Some(&mut session),
    )
    .unwrap();
    assert_eq!(
        restored,
        lower(&current, &raw, EntryPoint::None, false, false, 1, None).unwrap()
    );
    assert!(session.reused_machine_module);
    lower(
        &current,
        &raw,
        EntryPoint::None,
        false,
        false,
        1,
        Some(&mut session),
    )
    .unwrap();
    assert!(session.reused_machine_module);
}

fn history_fixture(first: i64, sibling: i64) -> VerifiedModule {
    let initial = fixture(first, TargetProfile::host_llvm(), &Limits::default());
    let mut module = initial.module().clone();
    module.functions[1].blocks[0].instructions[0].kind =
        InstructionKind::Constant(Constant::I64(sibling));
    recheck(module, &Limits::default())
}

#[test]
fn whole_machine_history_restores_function_entries_and_downstream_identity_together() {
    for scalar in [false, true] {
        let mut session = crate::emit::incremental::Session::default();
        let mut cold_identity = None;
        let mut broad_identity = None;
        for (index, (first, sibling)) in [(42, 99), (100, 200), (42, 99), (43, 99)]
            .into_iter()
            .enumerate()
        {
            let current = history_fixture(first, sibling);
            let raw = lir(&current);
            let actual = lower(
                &current,
                &raw,
                EntryPoint::None,
                scalar,
                false,
                1,
                Some(&mut session),
            )
            .unwrap();
            let fresh = lower(&current, &raw, EntryPoint::None, scalar, false, 1, None).unwrap();
            assert_eq!(actual, fresh);
            assert_eq!(session.reused_machine_module, index == 2);
            let identity = session.identity().unwrap();
            match index {
                0 => {
                    cold_identity = Some(identity);
                    assert_eq!(session.misses, 2);
                }
                1 => {
                    broad_identity = Some(identity);
                    assert_eq!(session.misses, 2);
                }
                2 => {
                    assert_eq!((session.hits, session.misses), (0, 0));
                    assert!(crate::incremental::same_identity(
                        &identity,
                        cold_identity.as_ref().unwrap()
                    ));
                    assert!(
                        !crate::incremental::same_identity(
                            &identity,
                            broad_identity.as_ref().unwrap()
                        ),
                        "restoring cold machine must never authorize broad transform/partition products"
                    );
                }
                3 => assert_eq!(
                    (session.hits, session.misses),
                    (1, 1),
                    "the old parent's entries, not broad entries, must feed the next private edit"
                ),
                _ => unreachable!(),
            }
        }
    }
}

#[test]
fn whole_machine_history_has_four_successful_lru_epochs_and_checkpoint_isolation() {
    let mut session = crate::emit::incremental::Session::default();
    let run = |value, session: &mut crate::emit::incremental::Session| {
        let current = history_fixture(value, 99);
        let raw = lir(&current);
        let actual = lower(
            &current,
            &raw,
            EntryPoint::None,
            false,
            false,
            1,
            Some(session),
        )
        .unwrap();
        let fresh = lower(&current, &raw, EntryPoint::None, false, false, 1, None).unwrap();
        assert_eq!(actual, fresh);
    };
    for value in [42, 43, 44, 45] {
        run(value, &mut session);
    }
    let mut checkpoint = session.clone();
    run(42, &mut session);
    assert!(
        session.reused_machine_module,
        "the fourth retained epoch remains available"
    );
    run(46, &mut session);
    run(42, &mut session);
    assert!(
        session.reused_machine_module,
        "a true LRU hit promotes the old parent"
    );
    run(43, &mut session);
    assert!(
        !session.reused_machine_module,
        "the fifth distinct epoch evicts the least recent"
    );
    run(43, &mut checkpoint);
    assert!(
        checkpoint.reused_machine_module,
        "pending publication cannot mutate checkpoint roots"
    );
    // A failed caller-owned transaction must not replace the successful epoch.
    let mut pending = checkpoint.clone();
    let current = history_fixture(900, 901);
    let raw = lir(&current);
    let bad = EntryPoint::ReturnValue(FunctionId(900));
    let failure = lower(&current, &raw, bad, false, false, 1, Some(&mut pending));
    assert!(failure.is_err());
    assert_eq!(failure, lower(&current, &raw, bad, false, false, 1, None));
    run(43, &mut checkpoint);
    assert!(checkpoint.reused_machine_module);
}

#[cfg(all(feature = "llvm-api", target_os = "linux"))]
#[test]
fn whole_machine_native_history_restore_and_next_private_edit_match_fresh_outputs() {
    let directory = TestDirectory::new();
    let mut options = BuildOptions::new(directory.0.join("program"));
    options.backend = NativeBackend::LlvmApi;
    options.optimization = Optimization::O2;
    options.incremental_o2 = true;
    options.codegen.units = 2;
    options.object_cache = Some(directory.0.join("objects"));
    options.emit.entry = EntryPoint::ReturnValue(FunctionId(0));
    let mut session = BackendSession::default();
    for (index, (first, sibling)) in [(42, 99), (100, 200), (42, 99), (43, 99)]
        .into_iter()
        .enumerate()
    {
        let current = history_fixture(first, sibling);
        let raw = lir(&current);
        let mut actual = build_view_with_cancellation_and_session(
            &current,
            &raw.as_view(),
            &options,
            &|| false,
            None,
            Some(&mut session),
        )
        .unwrap();
        actual.commit_object_cache();
        assert_eq!(session.summary().reused_machine_module, index == 2);
        if index == 2 {
            assert_eq!(
                (
                    session.summary().lowered_functions,
                    session.summary().reused_functions
                ),
                (0, 0)
            );
            assert!(
                !session.summary().reused_machine_transforms,
                "the last transformed product belongs to broad, not the restored cold epoch"
            );
        } else if index == 3 {
            assert_eq!(
                (
                    session.summary().lowered_functions,
                    session.summary().reused_functions
                ),
                (1, 1)
            );
        }
        let mut fresh_options = options.clone();
        fresh_options.object_cache = None;
        let fresh = build_view_with_cancellation_and_session(
            &current,
            &raw.as_view(),
            &fresh_options,
            &|| false,
            None,
            None,
        )
        .unwrap();
        assert_eq!(
            fs::read(&actual.llvm_ir).unwrap(),
            fs::read(&fresh.llvm_ir).unwrap()
        );
        assert_eq!(
            fs::read(&actual.executable).unwrap(),
            fs::read(&fresh.executable).unwrap()
        );
        let run = Command::new(&actual.executable).output().unwrap();
        assert!(run.status.success());
        assert_eq!(run.stdout, format!("{first}\n").as_bytes());
        assert!(run.stderr.is_empty());
    }
}

#[test]
fn machine_product_push_keeps_equal_root_and_patches_only_changed_result_leaves() {
    let limits = Limits::default();
    let original = fixture(42, TargetProfile::host_llvm(), &limits);
    let mut session = crate::emit::incremental::Session::default();
    let run = |verified: &VerifiedModule, session: &mut crate::emit::incremental::Session| {
        let raw = lir(verified);
        let actual = lower(
            verified,
            &raw,
            EntryPoint::None,
            false,
            false,
            1,
            Some(session),
        )
        .unwrap();
        let fresh = lower(verified, &raw, EntryPoint::None, false, false, 1, None).unwrap();
        assert_eq!(actual, fresh);
        zil_llvm_api::validate_module(&actual.0).unwrap();
        session.identity().unwrap()
    };
    let first = run(&original, &mut session);
    let mut module = original.module().clone();
    module.source_fingerprint = Fingerprint::of(b"same machine, new current source");
    module.functions[0].span = Span::new(module.functions[0].span.file, 1, 3);
    let relocated = recheck(module, &limits);
    let equal = run(&relocated, &mut session);
    assert!(
        !session.reused_machine_module,
        "actual function consumers must run"
    );
    assert_eq!((session.hits, session.misses), (2, 0));
    assert!(
        first.1.ptr_eq(&equal.1),
        "equal machine/source outputs keep the complete root"
    );
    let mut module = relocated.module().clone();
    module.functions[0].blocks[0].instructions[0].kind =
        InstructionKind::Constant(Constant::I64(43));
    let changed = recheck(module, &limits);
    let update = run(&changed, &mut session);
    assert_eq!((session.hits, session.misses), (1, 1));
    assert_eq!(update.1.changed_indices(&equal.1), vec![0]);
    assert!(!crate::incremental::same_identity(&equal, &update));
    assert!(std::sync::Arc::ptr_eq(
        equal.1.owner(1).unwrap(),
        update.1.owner(1).unwrap()
    ));
    // A retained parent restores both values and the exact downstream roots.
    let restored = run(&original, &mut session);
    assert!(session.reused_machine_module);
    assert!(first.1.ptr_eq(&restored.1));
}

#[test]
fn actual_output_pieces_reach_diagnostics_and_cgu_consumers_without_warm_flattening() {
    fn current(first: i64, second: i64) -> VerifiedModule {
        let verified = history_fixture(first, second);
        let mut module = verified.module().clone();
        module.functions[0].span.file = zil_base::FileId(0);
        module.functions[1].span.file = zil_base::FileId(1);
        recheck(module, &Limits::default())
    }
    fn produce(
        verified: &VerifiedModule,
        session: &mut crate::emit::incremental::Session,
    ) -> crate::machine_output::MachineOutput {
        crate::emit::lower_output_with_session(
            verified,
            &lir(verified).as_view(),
            &EmitOptions {
                entry: EntryPoint::None,
            },
            false,
            false,
            1,
            Some(session),
            true,
        )
        .unwrap()
        .0
    }
    let original = current(42, 99);
    let mut lower = crate::emit::incremental::Session::default();
    let cold = produce(&original, &mut lower);
    assert!(
        !cold.is_materialized(),
        "emitter must preserve actual pieces before any consumer"
    );
    let mut diagnostic = BackendSession::default();
    let cold_text = diagnostic.render_output(&cold);
    let (old, _, mut recipe) = crate::machine_cgu::partition_view_cached_with_recipe(
        cold.to_module(),
        &lir(&original).as_view(),
        EntryPoint::None,
        2,
        false,
        crate::machine_cgu::CallImportPolicy::Disabled,
        None,
    );
    assert_eq!(old.units.len(), 2);
    let mut recipe = recipe.take().unwrap();
    assert!(recipe.bind_output(&cold));
    assert_eq!(cold_text.to_text(), cold.to_module().to_text());
    let edited = current(43, 99);
    let warm = produce(&edited, &mut lower);
    assert_eq!((lower.hits, lower.misses), (1, 1));
    assert!(!warm.is_materialized());
    assert_eq!(warm.changed_items(&cold).unwrap().len(), 1);
    diagnostic.begin();
    let text = diagnostic.render_output(&warm);
    assert_eq!(diagnostic.summary().rendered_diagnostic_items, 1);
    let (updated, next, count) = recipe.replay_output(&warm, &old.units).unwrap();
    assert_eq!(count, 1);
    assert!(
        !warm.is_materialized(),
        "actual diag and CGU consumers must not flatten the current full machine"
    );
    let fresh = crate::emit::lower_view_with_session(
        &edited,
        &lir(&edited).as_view(),
        &EmitOptions {
            entry: EntryPoint::None,
        },
        false,
        false,
        1,
        None,
    )
    .unwrap()
    .0;
    assert_eq!(text.to_text(), fresh.to_text());
    let full = crate::machine_cgu::partition_view(
        fresh,
        &lir(&edited).as_view(),
        EntryPoint::None,
        2,
        false,
        false,
    );
    assert_eq!(updated.units, full.units);
    assert!(!warm.is_materialized());
    let broad = current(44, 100);
    let wide = produce(&broad, &mut lower);
    let (all, _, count) = next.replay_output(&wide, &updated.units).unwrap();
    assert_eq!(count, 2);
    assert!(!wide.is_materialized());
    let fresh = crate::emit::lower_view_with_session(
        &broad,
        &lir(&broad).as_view(),
        &EmitOptions {
            entry: EntryPoint::None,
        },
        false,
        false,
        1,
        None,
    )
    .unwrap()
    .0;
    assert_eq!(
        all.units,
        crate::machine_cgu::partition_view(
            fresh,
            &lir(&broad).as_view(),
            EntryPoint::None,
            2,
            false,
            false
        )
        .units
    );
    let restored = produce(&original, &mut lower);
    assert!(lower.reused_machine_module);
    assert_eq!(restored.changed_items(&cold), Some(vec![]));
    // Restoring a different parent is compared to its actual output tree, not
    // the last recipe's changed list or an unrelated lower identity.
    let (back, _, _) = next.replay_output(&restored, &updated.units).unwrap();
    assert_eq!(back.units, old.units);
}

#[cfg(all(feature = "llvm-api", target_os = "linux"))]
#[test]
fn native_output_pieces_preserve_current_checks_and_full_fresh_object_quality() {
    let directory = TestDirectory::new();
    let mut options = BuildOptions::new(directory.0.join("piece-program"));
    options.backend = NativeBackend::LlvmApi;
    options.optimization = Optimization::O2;
    options.codegen.units = 2;
    options.object_cache = Some(directory.0.join("piece-objects"));
    options.emit.entry = EntryPoint::ReturnValue(FunctionId(0));
    let mut session = BackendSession::default();
    for value in [42, 43] {
        let verified = history_fixture(value, 99);
        let mut module = verified.module().clone();
        module.functions[0].span.file = zil_base::FileId(0);
        module.functions[1].span.file = zil_base::FileId(1);
        let current = recheck(module, &Limits::default());
        let raw = lir(&current);
        let mut actual = build_view_with_cancellation_and_session(
            &current,
            &raw.as_view(),
            &options,
            &|| false,
            None,
            Some(&mut session),
        )
        .unwrap();
        actual.commit_object_cache();
        assert_eq!(actual.codegen.actual_units, 2);
        if value == 43 {
            assert!(session.summary().reused_partition_recipe);
            assert_eq!(session.summary().rebuilt_partition_units, 1);
            assert_eq!(session.summary().reused_partition_units, 1);
            assert!(
                !session
                    .transformed
                    .as_ref()
                    .unwrap()
                    .machine
                    .is_materialized(),
                "full native build must consume current pieces and actual per-unit Modules"
            );
        }
        let actual_ir = fs::read(&actual.llvm_ir).unwrap();
        let actual_elf = fs::read(&actual.executable).unwrap();
        let run = Command::new(&actual.executable).output().unwrap();
        assert!(run.status.success());
        assert_eq!(run.stdout, format!("{value}\n").as_bytes());
        assert!(run.stderr.is_empty());
        let mut fresh = options.clone();
        fresh.object_cache = None;
        let reference = build_view_with_cancellation_and_session(
            &current,
            &raw.as_view(),
            &fresh,
            &|| false,
            None,
            None,
        )
        .unwrap();
        assert_eq!(actual_ir, fs::read(reference.llvm_ir).unwrap());
        assert_eq!(actual_elf, fs::read(reference.executable).unwrap());
    }
}
