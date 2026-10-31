//! Small private-capacity controls isolate retention policy; driver controls use
//! real source and the production 64 MiB capacity.
use super::*;
use zil_base::{FileId, NodeId};

fn fixture(middle: usize) -> CoreProgram {
    CoreProgram {
        root: ModuleId(0),
        source_fingerprint: Fingerprint::ZERO,
        modules: [0, middle, 0]
            .into_iter()
            .enumerate()
            .map(|(index, extras)| {
                let span = Span::new(FileId(index as u32), 0, 1);
                let statements = (0..=extras)
                    .map(|i| {
                        let expression = CoreExpr {
                            id: NodeId((2 * i + 1) as u32),
                            span,
                            kind: CoreExprKind::Integer("1".into()),
                        };
                        CoreStatement {
                            id: NodeId((2 * i + 2) as u32),
                            span,
                            kind: if i == extras {
                                CoreStatementKind::Return(Some(expression))
                            } else {
                                CoreStatementKind::Expression(expression)
                            },
                        }
                    })
                    .collect();
                ProgramModule {
                    imports: vec![],
                    exports: vec![],
                    core: CoreModule {
                        schema_version: SCHEMA_VERSION,
                        module_id: ModuleId(index as u32),
                        source_fingerprint: Fingerprint::ZERO,
                        aggregates: vec![],
                        enums: vec![],
                        unsupported: vec![],
                        functions: vec![CoreFunction {
                            id: NodeId(0),
                            name: "f".into(),
                            type_parameters: vec![],
                            state_parameters: vec![],
                            callable_parameters: vec![],
                            callable_return: None,
                            foreign: None,
                            parameters: vec![],
                            return_type: Type::I32,
                            result_mode: PassingMode::Infer,
                            receiver: None,
                            span,
                            body: CoreBlock { statements, span },
                        }],
                    },
                }
            })
            .collect(),
    }
}

fn tables(program: &CoreProgram, limits: &Limits) -> Tables {
    let mut offset = 0;
    program
        .modules
        .iter()
        .map(|m| {
            let table = signatures(
                &m.core,
                limits,
                offset,
                Some(&m.exports.iter().copied().collect()),
            )
            .unwrap();
            offset += m.core.functions.len();
            (m.core.module_id, table)
        })
        .collect()
}

fn build(
    program: &CoreProgram,
    limits: &Limits,
    previous: &ProgramMemo,
    capacity: usize,
) -> CompileResult<(Vec<TypedFunction>, ProgramMemo)> {
    let tables = tables(program, limits);
    let target = TargetProfile::host_llvm();
    let mut request = Request::with_capacity(
        program,
        &tables,
        &target,
        limits,
        Fingerprint::ZERO,
        previous,
        capacity,
    )
    .unwrap();
    let mut state = closures::State::new(program.modules.len(), limits);
    let mut output = Vec::new();
    for (index, module) in program.modules.iter().enumerate() {
        let function = &module.core.functions[0];
        let before = state.memo_counters();
        let mut typed = if let Some(typed) = request.load(index, 0, &mut state) {
            typed
        } else {
            Builder::new(
                function,
                FunctionId(index as u32),
                &tables[&module.core.module_id],
                limits,
                &mut state,
            )
            .and_then(|builder| builder.finish(&function.body))
            .map_err(|e| vec![e])?
        };
        request
            .record(index, &typed, state.memo_usage(before))
            .unwrap();
        if module.core.module_id != program.root {
            typed.name = format!("@module{}:function{}", module.core.module_id.0, typed.id.0);
        }
        output.push(typed);
    }
    assert_eq!(request.stats.snapshot_cloned_functions, 0);
    let memo = request.finish(&output);
    Ok((output, memo))
}

fn two_small_capacity(program: &CoreProgram, memo: &ProgramMemo) -> usize {
    environment_weight(program, &TargetProfile::host_llvm()).unwrap()
        + [0, 2]
            .into_iter()
            .map(|i| {
                let entry = memo.modules[i].as_ref().unwrap();
                entry.source_weight + entry.typed_weight
            })
            .sum::<usize>()
}

#[test]
fn cp32_partial_selection_skips_large_middle_and_shares_unchanged_receipts() {
    let program = fixture(40);
    let limits = Limits::default();
    let expected = check_program(&program, &TargetProfile::host_llvm(), &limits).unwrap();
    let (_, full) = build(&program, &limits, &ProgramMemo::default(), MAX_WEIGHT).unwrap();
    let capacity = two_small_capacity(&program, &full);
    let (actual, partial) = build(&program, &limits, &ProgramMemo::default(), capacity).unwrap();
    assert_eq!(actual, expected.module.functions);
    assert!(
        partial.modules[0].is_some()
            && partial.modules[1].is_none()
            && partial.modules[2].is_some()
    );
    assert_eq!(partial.stats().retained_weight, capacity);
    assert_eq!(partial.stats().capacity_skipped_modules, 1);
    assert_eq!(partial.stats().snapshot_cloned_functions, 2);
    let (actual, next) = build(&program, &limits, &partial, capacity).unwrap();
    assert_eq!(actual, expected.module.functions);
    assert_eq!(next.stats().source_modules_reused, 2);
    assert_eq!(next.stats().source_modules_checked, 1);
    assert_eq!(next.stats().reused_functions, 2);
    assert_eq!(next.stats().checked_functions, 1);
    assert_eq!(next.stats().snapshot_cloned_functions, 0);
    for index in [0, 2] {
        assert!(Arc::ptr_eq(
            partial.modules[index].as_ref().unwrap(),
            next.modules[index].as_ref().unwrap()
        ));
        assert_eq!(
            next.modules[index].as_ref().unwrap().functions[0]
                .typed
                .name,
            "f"
        );
    }
}

#[test]
fn cp32_partial_receipt_does_not_hide_later_quota_failure() {
    let a = fixture(40);
    let b = fixture(41);
    let target = TargetProfile::host_llvm();
    for values in [false, true] {
        let limits = (1..512)
            .find_map(|n| {
                let mut limits = Limits::default();
                if values {
                    limits.max_values = n;
                } else {
                    limits.max_instructions = n;
                }
                let late = check_program(&b, &target, &limits)
                    .err()
                    .and_then(|e| e[0].span)
                    .is_some_and(|s| s.file == FileId(2));
                (late && check_program(&a, &target, &limits).is_ok()).then_some(limits)
            })
            .unwrap();
        let (_, full) = build(&a, &limits, &ProgramMemo::default(), MAX_WEIGHT).unwrap();
        let capacity = two_small_capacity(&a, &full);
        let (_, old) = build(&a, &limits, &ProgramMemo::default(), capacity).unwrap();
        let expected = check_program(&b, &target, &limits).unwrap_err();
        assert_eq!(expected[0].span.unwrap().file, FileId(2));
        assert_eq!(build(&b, &limits, &old, capacity).err().unwrap(), expected);
        let (restored, next) = build(&a, &limits, &old, capacity).unwrap();
        assert_eq!(
            restored,
            check_program(&a, &target, &limits)
                .unwrap()
                .module
                .functions
        );
        assert_eq!(next.stats().reused_functions, 2);
    }
}

#[test]
fn cp32_partial_selection_still_checks_complete_identity_and_oversized_body_tail() {
    let a = fixture(40);
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let (_, full) = build(&a, &limits, &ProgramMemo::default(), MAX_WEIGHT).unwrap();
    let capacity = two_small_capacity(&a, &full);
    let (_, partial) = build(&a, &limits, &ProgramMemo::default(), capacity).unwrap();
    let mut changed = a.clone();
    let CoreStatementKind::Return(Some(expr)) =
        &mut changed.modules[0].core.functions[0].body.statements[0].kind
    else {
        unreachable!()
    };
    expr.kind = CoreExprKind::Integer("9".into()); // Preserve all old IDs/spans/hashes.
    let (actual, next) = build(&changed, &limits, &partial, capacity).unwrap();
    assert_eq!(
        actual,
        check_program(&changed, &target, &limits)
            .unwrap()
            .module
            .functions
    );
    assert_eq!(next.stats().source_modules_reused, 1);
    assert_eq!(next.stats().snapshot_cloned_functions, 1);
    let table = tables(&a, &limits);
    assert!(
        Request::with_capacity(
            &a,
            &table,
            &target,
            &limits,
            Fingerprint::ZERO,
            &partial,
            environment_weight(&a, &target).unwrap() - 1
        )
        .is_none()
    );
    let mut invalid_tail = a.clone();
    let last = invalid_tail.modules[1].core.functions[0]
        .body
        .statements
        .last_mut()
        .unwrap();
    let CoreStatementKind::Return(Some(expr)) = &mut last.kind else {
        unreachable!()
    };
    expr.kind = CoreExprKind::Ascribed {
        ty: Type::Unsupported("late unsupported type".into()),
        value: Box::new(expr.clone()),
    };
    assert!(
        Request::with_capacity(
            &invalid_tail,
            &table,
            &target,
            &limits,
            Fingerprint::ZERO,
            &partial,
            capacity
        )
        .is_none()
    );
}
