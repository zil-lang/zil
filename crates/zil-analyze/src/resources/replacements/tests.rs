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
