use super::*;
use crate::place_replacements::fixture;
fn with_caller(fresh: bool) -> TypedModule {
    fixture::module_with_relay_and_caller(fresh)
}

#[test]
fn replacement_transfer_updates_actual_field_birth_through_two_calls() {
    for fresh in [false, true] {
        let module = with_caller(fresh);
        let graphs = vec![(vec![vec![]], vec![true]); 3];
        let limits = Limits::default();
        let mut b = Budget::new(&limits);
        b.head_descriptions = true;
        let program = Program::new(&module, &graphs, &mut b).unwrap();
        let effects = infer(&module, &graphs, &program, &mut b).unwrap();
        let target = Target {
            root: origins::Root::Parameter(0),
            path: vec![PlaceProjection::Field(0)],
            mutable: true,
        };
        assert_eq!(
            effects[0].replacements[&target],
            BTreeSet::from([Birth::Argument(1)])
        );
        assert_eq!(
            effects[1].replacements[&target],
            BTreeSet::from([Birth::Argument(1)])
        );
        let result = analyze_function_program(
            &module,
            &module.functions[2],
            &[true],
            &effects,
            true,
            &mut b,
            None,
            Some(&program),
        );
        if fresh {
            result.unwrap();
        } else {
            assert!(result.unwrap_err().iter().any(|d| {
                d.message
                    .contains("create result forwards an incoming responsibility")
            }));
        }
    }
}

#[test]
fn replacement_through_capture_keeps_real_fnmut_authority() {
    for kind in [CallableKind::Fn, CallableKind::FnMut] {
        let mut module = fixture::module();
        let address = module.functions[0].locals[0].ty.clone();
        let resource = module.functions[0].locals[1].ty.clone();
        let f = &mut module.functions[0];
        f.parameters = vec![LocalId(1)];
        f.environment = Some(ClosureId(0));
        f.blocks[0].instructions[0].kind = InstructionKind::ReadCapture { index: 0 };
        module.closures.push(ClosureDefinition {
            storage_life: None,
            id: ClosureId(0),
            function: FunctionId(0),
            captures: vec![address],
            parameters: vec![resource],
            result: Type::Unit,
            kind,
            lending_captures: vec![],
        });
        let graphs = vec![(vec![vec![]], vec![true])];
        let limits = Limits::default();
        let mut b = Budget::new(&limits);
        b.head_descriptions = true;
        let program = Program::new(&module, &graphs, &mut b).unwrap();
        let result = infer(&module, &graphs, &program, &mut b);
        if kind == CallableKind::Fn {
            assert!(
                result
                    .unwrap_err()
                    .iter()
                    .any(|d| d.message.contains("shared Fn entry cannot replace"))
            );
        } else {
            let effects = result.unwrap();
            assert!(effects[0].borrowed_fields.contains(&0));
            assert!(
                effects[0]
                    .replacements
                    .keys()
                    .any(|at| at.root == origins::Root::Capture(0))
            );
        }
    }
}

#[test]
fn replacement_birth_frontier_has_a_repeatable_live_budget_boundary() {
    let module = with_caller(true);
    let graphs = vec![(vec![vec![]], vec![true]); 3];
    let run = |work| -> CompileResult<()> {
        let limits = Limits {
            max_analysis_steps: work,
            ..Limits::default()
        };
        let mut b = Budget::new(&limits);
        b.head_descriptions = true;
        let program = Program::new(&module, &graphs, &mut b)?;
        let effects = infer(&module, &graphs, &program, &mut b)?;
        for (f, (_, reachable)) in module.functions.iter().zip(&graphs) {
            analyze_function_program(
                &module,
                f,
                reachable,
                &effects,
                true,
                &mut b,
                None,
                Some(&program),
            )?;
        }
        Ok(())
    };
    let (mut lo, mut hi) = (1, Limits::default().max_analysis_steps);
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        if run(mid).is_ok() {
            hi = mid
        } else {
            lo = mid + 1
        }
    }
    run(lo).unwrap();
    assert!(
        run(lo - 1)
            .unwrap_err()
            .iter()
            .all(|d| d.category == zil_base::ResultCategory::ResourceLimit)
    );
    run(lo).unwrap();
    println!("340 Analyze replacement birth/actual-route frontier N={lo}");
}

// Actual component IR, independent of Source formation. The caller's old leaf
// is Incoming in every case, so a fresh result needs the actual helper Commit.
fn whole_helper(incoming: bool, conditional: bool) -> TypedModule {
    use fixture::instruction as i;
    use zil_ir::{BasicBlock, Constant, ReservationId};
    let mut module = fixture::module_with_relay_and_caller(true);
    module.functions.remove(1);
    let resource = module.functions[0].locals[1].ty.clone();
    let Type::Address { inner, .. } = &module.functions[0].locals[0].ty else {
        unreachable!()
    };
    let package = inner.as_ref().clone();
    let helper = &mut module.functions[0];
    if !incoming {
        helper.parameters.pop();
    }
    helper.values.extend([Type::I32, package]);
    helper.blocks[0].instructions = vec![
        i(Some(ValueId(0)), InstructionKind::Load(LocalId(0))),
        i(
            None,
            InstructionKind::ReservePlaceAddress {
                reservation: ReservationId(71),
                address: ValueId(0),
                path: vec![],
            },
        ),
        i(
            Some(ValueId(2)),
            InstructionKind::Constant(Constant::I32(66)),
        ),
        i(
            Some(ValueId(1)),
            if incoming {
                InstructionKind::TakeLocal { local: LocalId(1) }
            } else {
                InstructionKind::MakeManaged {
                    fields: vec![ValueId(2)],
                }
            },
        ),
        i(
            Some(ValueId(3)),
            InstructionKind::MakePackage {
                fields: vec![ValueId(1), ValueId(2)],
            },
        ),
        i(
            None,
            InstructionKind::CommitPlaceReplace {
                reservation: ReservationId(71),
                value: ValueId(3),
            },
        ),
    ];
    if conditional {
        assert!(!incoming);
        let write = std::mem::take(&mut helper.blocks[0].instructions);
        helper.values.push(Type::Bool);
        helper.blocks[0].instructions = vec![i(
            Some(ValueId(4)),
            InstructionKind::Constant(Constant::Bool(true)),
        )];
        helper.blocks[0].terminator = Terminator::Branch {
            condition: ValueId(4),
            then_block: BlockId(1),
            else_block: BlockId(2),
        };
        helper.blocks.extend([
            BasicBlock {
                id: BlockId(1),
                instructions: write,
                terminator: Terminator::Return(None),
            },
            BasicBlock {
                id: BlockId(2),
                instructions: vec![],
                terminator: Terminator::Return(None),
            },
        ]);
    }
    let caller = &mut module.functions[1];
    caller.id = FunctionId(1);
    caller.parameters = vec![LocalId(0)];
    caller.blocks[0].instructions[1].kind = InstructionKind::TakeLocal { local: LocalId(0) };
    if incoming {
        let mut argument = caller.locals[0].clone();
        argument.id = LocalId(2);
        argument.name = "rhs".into();
        argument.ty = resource;
        caller.locals.push(argument);
        caller.parameters.push(LocalId(2));
        caller.blocks[0].instructions[5].kind = InstructionKind::TakeLocal { local: LocalId(2) };
    } else {
        caller.blocks[0].instructions.remove(5);
    }
    let call = caller.blocks[0]
        .instructions
        .iter_mut()
        .find(|i| matches!(i.kind, InstructionKind::Call { .. }))
        .unwrap();
    call.kind = InstructionKind::Call {
        function: FunctionId(0),
        arguments: if incoming {
            vec![ValueId(3), ValueId(4)]
        } else {
            vec![ValueId(3)]
        },
    };
    module
}

#[test]
fn whole_helper_commit_keeps_actual_rhs_and_conditional_may_distinct() {
    for (incoming, conditional) in [(false, false), (true, false), (false, true)] {
        let module = whole_helper(incoming, conditional);
        let helper_graph = if conditional {
            (vec![vec![], vec![0], vec![0]], vec![true; 3])
        } else {
            (vec![vec![]], vec![true])
        };
        let graphs = vec![helper_graph, (vec![vec![]], vec![true])];
        let limits = Limits::default();
        let mut b = Budget::new(&limits);
        b.head_descriptions = true;
        let program = Program::new(&module, &graphs, &mut b).unwrap();
        let effects = infer(&module, &graphs, &program, &mut b).unwrap();
        let target = Target {
            root: origins::Root::Parameter(0),
            path: vec![PlaceProjection::Field(0)],
            mutable: true,
        };
        assert!(effects[0].replacements.contains_key(&target));
        if conditional {
            assert!(effects[0].whole_commits.is_none());
        } else {
            assert_eq!(
                effects[0].whole_commits.as_ref().unwrap()[&target],
                BTreeSet::from([if incoming {
                    Birth::Argument(1)
                } else {
                    Birth::Created
                }])
            );
        }
        let result = analyze_function_program(
            &module,
            &module.functions[1],
            &[true],
            &effects,
            true,
            &mut b,
            None,
            Some(&program),
        );
        if incoming || conditional {
            assert!(result.unwrap_err().iter().any(|d| {
                d.message
                    .contains("create result forwards an incoming responsibility")
            }));
            let mut give = module.clone();
            give.functions[1].result_mode = PassingMode::Give;
            analyze_function_program(
                &give,
                &give.functions[1],
                &[true],
                &effects,
                true,
                &mut b,
                None,
                Some(&program),
            )
            .unwrap();
        } else {
            result.unwrap();
        }
    }
}

#[test]
fn whole_helper_later_field_commit_or_helper_clears_the_must_fact() {
    use fixture::instruction as i;
    use zil_ir::ReservationId;
    for nested in [false, true] {
        let mut module = whole_helper(false, false);
        let resource = module.functions[0].locals[1].ty.clone();
        if nested {
            let mut mutator = fixture::module().functions.remove(0);
            mutator.id = FunctionId(2);
            module.functions.push(mutator);
        }
        let helper = &mut module.functions[0];
        helper.values.push(resource);
        helper.blocks[0].instructions.push(i(
            Some(ValueId(4)),
            InstructionKind::MakeManaged {
                fields: vec![ValueId(2)],
            },
        ));
        if nested {
            helper.blocks[0].instructions.push(i(
                None,
                InstructionKind::Call {
                    function: FunctionId(2),
                    arguments: vec![ValueId(0), ValueId(4)],
                },
            ));
        } else {
            helper.blocks[0].instructions.extend([
                i(
                    None,
                    InstructionKind::ReservePlaceAddress {
                        reservation: ReservationId(72),
                        address: ValueId(0),
                        path: vec![PlaceProjection::Field(0)],
                    },
                ),
                i(
                    None,
                    InstructionKind::CommitPlaceReplace {
                        reservation: ReservationId(72),
                        value: ValueId(4),
                    },
                ),
            ]);
        }
        let graphs = vec![(vec![vec![]], vec![true]); module.functions.len()];
        let limits = Limits::default();
        let mut b = Budget::new(&limits);
        b.head_descriptions = true;
        let program = Program::new(&module, &graphs, &mut b).unwrap();
        let effects = infer(&module, &graphs, &program, &mut b).unwrap();
        assert!(effects[0].whole_commits.as_ref().unwrap().is_empty());
        let errors = analyze_function_program(
            &module,
            &module.functions[1],
            &[true],
            &effects,
            true,
            &mut b,
            None,
            Some(&program),
        )
        .unwrap_err();
        assert!(
            errors.iter().any(|d| d
                .message
                .contains("create result forwards an incoming responsibility")),
            "nested={nested}: {errors:?}"
        );
    }
}

// Exercise the actual resource transfer, not just result-shape classification.
// 0: scalar return; 1: ok after Commit; 2: err after Commit; 3: err after Abort.
fn copy_result_helper(mode: u8) -> TypedModule {
    use fixture::instruction as i;
    use zil_ir::{Constant, ReservationId};
    let mut module = whole_helper(false, false);
    let helper = &mut module.functions[0];
    if mode == 3 {
        helper.blocks[0].instructions.truncate(4);
        helper.blocks[0].instructions.extend([
            i(
                None,
                InstructionKind::Store {
                    local: LocalId(1),
                    value: ValueId(1),
                },
            ),
            i(
                None,
                InstructionKind::AbortPlaceReservation {
                    reservation: ReservationId(71),
                },
            ),
            i(None, InstructionKind::DropManaged { local: LocalId(1) }),
        ]);
    }
    let (result, returned) = if mode == 0 {
        (Type::I32, ValueId(2))
    } else {
        let union = Type::ErrorUnion {
            error: Box::new(Type::Unit),
            result: Box::new(Type::I32),
        };
        helper.values.extend([Type::Unit, union.clone()]);
        helper.blocks[0].instructions.extend([
            i(Some(ValueId(4)), InstructionKind::Constant(Constant::Unit)),
            i(
                Some(ValueId(5)),
                InstructionKind::MakeErrorUnion {
                    is_error: mode >= 2,
                    value: if mode >= 2 { ValueId(4) } else { ValueId(2) },
                },
            ),
        ]);
        (union, ValueId(5))
    };
    helper.return_type = result.clone();
    helper.blocks[0].terminator = Terminator::Return(Some(returned));
    let caller = &mut module.functions[1];
    let output = ValueId(caller.values.len() as u32);
    caller.values.push(result);
    caller.blocks[0]
        .instructions
        .iter_mut()
        .find(|i| matches!(i.kind, InstructionKind::Call { .. }))
        .unwrap()
        .result = Some(output);
    module
}

fn run_copy_result_helper(module: &TypedModule, work: usize) -> CompileResult<usize> {
    let graphs = vec![(vec![vec![]], vec![true]); module.functions.len()];
    let limits = Limits {
        max_analysis_steps: work,
        ..Limits::default()
    };
    let mut b = Budget::new(&limits);
    b.head_descriptions = true;
    let program = Program::new(module, &graphs, &mut b)?;
    let effects = infer(module, &graphs, &program, &mut b)?;
    assert!(effects[0].whole_commits.is_none());
    for (f, (_, reachable)) in module.functions.iter().zip(graphs) {
        analyze_function_program(
            module,
            f,
            &reachable,
            &effects,
            true,
            &mut b,
            None,
            Some(&program),
        )?;
    }
    Ok(work - b.remaining())
}

#[test]
fn whole_copy_result_commit_and_abort_preserve_may_origin_and_real_cleanup() {
    for mode in 0..4 {
        let mut module = copy_result_helper(mode);
        let graphs = vec![(vec![vec![]], vec![true]); 2];
        let limits = Limits::default();
        let mut b = Budget::new(&limits);
        b.head_descriptions = true;
        let program = Program::new(&module, &graphs, &mut b).unwrap();
        let effects = infer(&module, &graphs, &program, &mut b).unwrap();
        assert!(effects[0].whole_commits.is_none());
        assert_eq!(effects[0].replacements.is_empty(), mode == 3);
        let errors = analyze_function_program(
            &module,
            &module.functions[1],
            &[true],
            &effects,
            true,
            &mut b,
            None,
            Some(&program),
        )
        .unwrap_err();
        assert!(
            errors.iter().any(|d| d
                .message
                .contains("create result forwards an incoming responsibility")),
            "mode={mode}: {errors:?}"
        );
        module.functions[1].result_mode = PassingMode::Give;
        run_copy_result_helper(&module, limits.max_analysis_steps).unwrap();
    }
    let mut missing_abort = copy_result_helper(3);
    missing_abort.functions[0].blocks[0]
        .instructions
        .retain(|i| !matches!(i.kind, InstructionKind::AbortPlaceReservation { .. }));
    assert!(
        run_copy_result_helper(&missing_abort, Limits::default().max_analysis_steps)
            .unwrap_err()
            .iter()
            .any(|d| d.message.contains("prepared replacement activation"))
    );
    let mut leaked_preparation = copy_result_helper(3);
    leaked_preparation.functions[0].blocks[0]
        .instructions
        .retain(|i| !matches!(i.kind, InstructionKind::DropManaged { .. }));
    assert!(
        run_copy_result_helper(&leaked_preparation, Limits::default().max_analysis_steps)
            .unwrap_err()
            .iter()
            .any(|d| d.message.contains("responsibility unpaid"))
    );
}

#[test]
fn whole_copy_result_qualification_does_not_admit_copy_code_or_foreign_carriers() {
    let ordinary = Type::ErrorUnion {
        error: Box::new(Type::Enum {
            name: "Failure".into(),
            variants: vec![("Bad".into(), vec![Type::I32])],
        }),
        result: Box::new(Type::Tuple(vec![
            Type::Record {
                name: "Report".into(),
                fields: vec![("code".into(), Type::I64)],
            },
            Type::Array {
                element: Box::new(Type::Bool),
                length: 3,
            },
        ])),
    };
    let limits = Limits::default();
    for (ty, admitted) in [
        (ordinary, true),
        (
            Type::FunctionPointer {
                parameters: vec![],
                result: Box::new(Type::Unit),
            },
            false,
        ),
        (Type::ForeignHandle, false),
        (Type::Tuple(vec![Type::CWideString]), false),
        (
            Type::ErrorUnion {
                error: Box::new(Type::Address {
                    mutable: false,
                    inner: Box::new(Type::I32),
                }),
                result: Box::new(Type::I32),
            },
            false,
        ),
    ] {
        let mut module = copy_result_helper(0);
        module.functions[0].return_type = ty;
        let result = Program::new(
            &module,
            &vec![(vec![vec![]], vec![true]); 2],
            &mut Budget::new(&limits),
        );
        assert_eq!(result.is_ok(), admitted);
        if !admitted {
            assert!(
                result
                    .err()
                    .unwrap()
                    .iter()
                    .all(|d| d.category == zil_base::ResultCategory::Unsupported)
            );
        }
    }
}

#[test]
fn whole_copy_result_request_work_is_exact_and_retryable() {
    let limits = Limits::default();
    for mode in [1, 3] {
        let mut module = copy_result_helper(mode);
        module.functions[1].result_mode = PassingMode::Give;
        let work = run_copy_result_helper(&module, limits.max_analysis_steps).unwrap();
        assert_eq!(run_copy_result_helper(&module, work).unwrap(), work);
        assert!(
            run_copy_result_helper(&module, work - 1)
                .unwrap_err()
                .iter()
                .all(|d| d.category == zil_base::ResultCategory::ResourceLimit)
        );
        assert_eq!(run_copy_result_helper(&module, work).unwrap(), work);
        println!("Analyze actual copy-result helper mode={mode} N={work}");
    }
}

fn compare_replacement_schedulers(module: &TypedModule, graphs: &[(Vec<Vec<usize>>, Vec<bool>)]) {
    let run = |frontier| {
        let mut budget = Budget::new(&Limits::default());
        budget.head_descriptions = true;
        let program = Program::new(module, graphs, &mut budget)?;
        if frontier {
            infer(module, graphs, &program, &mut budget)
        } else {
            infer_full_scan(module, graphs, &program, &mut budget)
        }
    };
    assert_eq!(run(true), run(false));
}

#[test]
fn dirty_replacement_summaries_match_exhaustive_rows_through_recursive_calls() {
    use zil_ir::{BasicBlock, BlockId};
    for fresh in [false, true] {
        let module = with_caller(fresh);
        compare_replacement_schedulers(&module, &vec![(vec![vec![]], vec![true]); 3]);
        for recursive in [1, 3] {
            let mut module = module.clone();
            let mut cycle = module.functions[1].clone();
            cycle.id = FunctionId(3);
            cycle.name = "recursive_replacement_relay".into();
            cycle.values.push(Type::Bool);
            cycle.blocks[0].instructions.pop();
            cycle.blocks[0].instructions.push(fixture::instruction(
                Some(ValueId(2)),
                InstructionKind::Constant(Constant::Bool(true)),
            ));
            cycle.blocks[0].terminator = Terminator::Branch {
                condition: ValueId(2),
                then_block: BlockId(1),
                else_block: BlockId(2),
            };
            for (id, callee) in [(1, 0), (2, recursive)] {
                cycle.blocks.push(BasicBlock {
                    id: BlockId(id),
                    instructions: vec![fixture::instruction(
                        None,
                        InstructionKind::Call {
                            function: FunctionId(callee),
                            arguments: vec![ValueId(0), ValueId(1)],
                        },
                    )],
                    terminator: Terminator::Return(None),
                });
            }
            module.functions[1].blocks[0]
                .instructions
                .last_mut()
                .unwrap()
                .kind = InstructionKind::Call {
                function: FunctionId(3),
                arguments: vec![ValueId(0), ValueId(1)],
            };
            module.functions.push(cycle);
            let graphs = vec![
                (vec![vec![]], vec![true]),
                (vec![vec![]], vec![true]),
                (vec![vec![]], vec![true]),
                (vec![vec![], vec![0], vec![0]], vec![true; 3]),
            ];
            compare_replacement_schedulers(&module, &graphs);
            // Change physical function order and every actual direct target;
            // a source-order seed must not be mistaken for fixed-point closure.
            let order = [3usize, 2, 1, 0];
            let mut inverse = [0usize; 4];
            for (new, old) in order.iter().copied().enumerate() {
                inverse[old] = new;
            }
            let mut permuted = module.clone();
            permuted.functions = order
                .iter()
                .map(|&old| module.functions[old].clone())
                .collect();
            for (index, f) in permuted.functions.iter_mut().enumerate() {
                f.id = FunctionId(index as u32);
                for block in &mut f.blocks {
                    for i in &mut block.instructions {
                        if let InstructionKind::Call { function, .. } = &mut i.kind {
                            *function = FunctionId(inverse[function.0 as usize] as u32);
                        }
                    }
                }
            }
            let permuted_graphs: Vec<_> = order.iter().map(|&old| graphs[old].clone()).collect();
            compare_replacement_schedulers(&permuted, &permuted_graphs);
        }
    }
    for incoming in [false, true] {
        let module = whole_helper(incoming, false);
        compare_replacement_schedulers(&module, &vec![(vec![vec![]], vec![true]); 2]);
    }
}

fn complete_scheduler_run(
    module: &TypedModule,
    graphs: &[(Vec<Vec<usize>>, Vec<bool>)],
    mode: u8,
) -> CompileResult<(Vec<Effect>, Vec<retirement::Pairs>)> {
    let mut b = Budget::new(&Limits::default());
    b.head_descriptions = true;
    // These raw component fixtures explicitly contain the actual producer
    // instructions; final validation/pair closure must not be disabled merely
    // because this helper bypasses the full public Analyze preflight wrapper.
    b.replacement_functions = module
        .functions
        .iter()
        .filter(|f| {
            f.blocks
                .iter()
                .flat_map(|block| &block.instructions)
                .any(|i| matches!(i.kind, InstructionKind::CommitPlaceReplace { .. }))
        })
        .map(|f| f.id)
        .collect();
    let program = Program::new(module, graphs, &mut b)?;
    let effects = match mode {
        0 => infer(module, graphs, &program, &mut b)?,
        1 => infer_full_scan(module, graphs, &program, &mut b)?,
        _ => {
            fee(&mut b, module.functions.len())?;
            let mut missing = effect_consumers_with_public(module, graphs, true, &mut b)?;
            // Deliberately lose every notification, including the ordering
            // hint. Only the original complete stable sweep can repair it.
            for row in &mut missing {
                row.clear();
            }
            infer_from_consumers(
                module,
                graphs,
                &program,
                vec![Effect::default(); module.functions.len()],
                missing,
                &mut b,
            )?
        }
    };
    for (f, (_, reachable)) in module.functions.iter().zip(graphs) {
        analyze_function_program(
            module,
            f,
            reachable,
            &effects,
            true,
            &mut b,
            None,
            Some(&program),
        )?;
    }
    program.close_retirement_pairs(module, graphs, &mut b)?;
    let pairs = program.retirement_pairs.borrow().clone();
    Ok((effects, pairs))
}
fn compare_complete_schedulers(module: &TypedModule, graphs: &[(Vec<Vec<usize>>, Vec<bool>)]) {
    let old = complete_scheduler_run(module, graphs, 1);
    for mode in [0, 2] {
        let new = complete_scheduler_run(module, graphs, mode);
        match (&new, &old) {
            (Ok(a), Ok(b)) => assert_eq!(a, b, "complete effects/must/final retirement pairs"),
            (Err(a), Err(b)) => {
                assert!(
                    a.iter()
                        .chain(b)
                        .all(|d| d.category != zil_base::ResultCategory::ResourceLimit),
                    "a shared budget failure is not semantic equivalence"
                );
                assert_eq!(
                    a.iter()
                        .map(|d| format!("{:?}", d.category))
                        .collect::<BTreeSet<_>>(),
                    b.iter()
                        .map(|d| format!("{:?}", d.category))
                        .collect::<BTreeSet<_>>()
                );
            }
            _ => panic!("new={new:?} old={old:?}"),
        }
    }
}
#[test]
fn initial_callee_order_is_a_complete_fair_permutation_for_every_small_graph() {
    for count in 0..=3 {
        for mask in 0..(1usize << (count * count)) {
            let mut graph = vec![Vec::new(); count];
            for (callee, callers) in graph.iter_mut().enumerate() {
                for caller in 0..count {
                    if mask & (1 << (callee * count + caller)) != 0 {
                        callers.push(caller);
                    }
                }
            }
            let order = initial_callee_order(&graph, &mut Budget::new(&Limits::default())).unwrap();
            assert_eq!(order.len(), count);
            assert_eq!(
                order.iter().copied().collect::<BTreeSet<_>>(),
                (0..count).collect()
            );
        }
    }
    let dag = vec![vec![], vec![0], vec![1], vec![1]];
    assert_eq!(
        initial_callee_order(&dag, &mut Budget::new(&Limits::default())).unwrap(),
        VecDeque::from([2, 3, 1, 0])
    );
    // Self/mutual cycles and a caller waiting on the cycle all remain present
    // in original ID order; no new SCC solver or missed-function assumption.
    let cyclic = vec![vec![1], vec![0, 2], vec![], vec![3]];
    assert_eq!(
        initial_callee_order(&cyclic, &mut Budget::new(&Limits::default())).unwrap(),
        VecDeque::from([0, 1, 2, 3])
    );
    assert!(initial_callee_order(&[vec![9]], &mut Budget::new(&Limits::default())).is_err());
}
#[test]
fn callee_order_complete_validation_and_pair_closure_match_the_exhaustive_oracle() {
    for fresh in [false, true] {
        let original = with_caller(fresh);
        let mut reverse = original.clone();
        reverse.functions = original.functions.iter().rev().cloned().collect();
        let count = reverse.functions.len();
        for (index, f) in reverse.functions.iter_mut().enumerate() {
            f.id = FunctionId(index as u32);
            for block in &mut f.blocks {
                for i in &mut block.instructions {
                    if let InstructionKind::Call { function, .. } = &mut i.kind {
                        function.0 = (count - 1 - function.0 as usize) as u32;
                    }
                }
            }
        }
        for module in [original, reverse] {
            if fresh {
                let graphs = vec![(vec![vec![]], vec![true]); module.functions.len()];
                for mode in [0, 1, 2] {
                    complete_scheduler_run(&module, &graphs, mode).unwrap();
                }
            }
            let graphs = vec![(vec![vec![]], vec![true]); module.functions.len()];
            compare_complete_schedulers(&module, &graphs);
        }
    }
    for incoming in [false, true] {
        let module = whole_helper(incoming, false);
        compare_complete_schedulers(&module, &vec![(vec![vec![]], vec![true]); 2]);
    }
}
#[test]
fn captured_replacement_and_ordinary_rhs_keep_the_final_authority_rejection() {
    for kind in [CallableKind::Fn, CallableKind::FnMut] {
        let mut module = fixture::module();
        let address = module.functions[0].locals[0].ty.clone();
        let resource = module.functions[0].locals[1].ty.clone();
        let f = &mut module.functions[0];
        f.parameters = vec![LocalId(1)];
        f.environment = Some(ClosureId(0));
        f.blocks[0].instructions[0].kind = InstructionKind::ReadCapture { index: 0 };
        module.closures.push(ClosureDefinition {
            storage_life: None,
            id: ClosureId(0),
            function: FunctionId(0),
            captures: vec![address],
            parameters: vec![resource],
            result: Type::Unit,
            kind,
            lending_captures: vec![],
        });
        compare_complete_schedulers(&module, &[(vec![vec![]], vec![true])]);
        if kind == CallableKind::FnMut {
            for mode in [0, 1, 2] {
                complete_scheduler_run(&module, &[(vec![vec![]], vec![true])], mode).unwrap();
            }
        }
        if kind == CallableKind::Fn {
            assert!(
                complete_scheduler_run(&module, &[(vec![vec![]], vec![true])], 0)
                    .unwrap_err()
                    .iter()
                    .any(|d| d.message.contains("shared Fn entry cannot replace"))
            );
        }
    }
}

#[test]
fn initial_order_pays_an_exact_fresh_request_boundary() {
    let graph = vec![vec![], vec![0], vec![1], vec![1, 4], vec![3]];
    let run = |steps| {
        initial_callee_order(
            &graph,
            &mut Budget::new(&Limits {
                max_analysis_steps: steps,
                ..Limits::default()
            }),
        )
    };
    let (mut lo, mut hi) = (1, Limits::default().max_analysis_steps);
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        if run(mid).is_ok() {
            hi = mid
        } else {
            lo = mid + 1
        }
    }
    let order = run(lo).unwrap();
    assert_eq!(order.len(), graph.len());
    assert!(
        run(lo - 1)
            .unwrap_err()
            .iter()
            .all(|d| d.category == zil_base::ResultCategory::ResourceLimit)
    );
    assert_eq!(run(lo).unwrap(), order);
    println!("Replacement initial fair order N={lo}; N-1 rejects/fresh N repeats");
}
