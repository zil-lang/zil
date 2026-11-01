//! Finite Module regressions for collecting and validation-only preflight.
//! No LLVM installation, archived fixture, or exhaustive graph generator needed.
use super::*;

fn inst(name: Option<&str>, op: Op) -> Inst {
    Inst::new(name.map(str::to_owned), op)
}

fn block(name: &str, instructions: Vec<Inst>) -> Block {
    Block {
        name: name.into(),
        instructions,
    }
}

fn function_item(name: &str, blocks: Vec<Block>) -> Item {
    Item::Function(Function {
        name: name.into(),
        linkage: Linkage::External,
        signature: Signature::new(Ty::Int(32), vec![]),
        attrs: vec![],
        blocks: Some(blocks),
    })
}

fn module(items: Vec<Item>) -> Module {
    Module {
        triple: "x86_64-unknown-linux-gnu".into(),
        fingerprint: "validate-only-ci".into(),
        items,
    }
}

fn ret(value: Val) -> Inst {
    inst(None, Op::Return(Some(Arg::new(Ty::Int(32), value))))
}

fn add(name: &str, left: Val, right: Val) -> Inst {
    inst(
        Some(name),
        Op::Binary {
            opcode: "add".into(),
            ty: Ty::Int(32),
            left,
            right,
        },
    )
}

fn branch(label: &str) -> Inst {
    inst(None, Op::Branch(label.into()))
}

fn body_mut(module: &mut Module) -> &mut Vec<Block> {
    let Item::Function(function) = &mut module.items[0] else {
        panic!("fixture function")
    };
    function.blocks.as_mut().expect("fixture definition")
}

fn accepted(module: &Module) -> BTreeMap<String, Vec<(usize, usize)>> {
    let collecting = validate_impl::<true>(module).expect("specified valid Module");
    let validation_only = validate_impl::<false>(module).expect("same valid Module");
    assert!(
        validation_only.functions.is_empty(),
        "validation-only must not retain schedules"
    );
    for item in &module.items {
        let Item::Function(function) = item else {
            continue;
        };
        let positions = &collecting.functions[&function.name];
        let expected = function
            .blocks
            .iter()
            .flatten()
            .enumerate()
            .flat_map(|(b, block)| {
                block
                    .instructions
                    .iter()
                    .enumerate()
                    .filter_map(move |(j, inst)| {
                        (!matches!(inst.op, Op::Phi { .. })).then_some((b, j))
                    })
            })
            .collect::<BTreeSet<_>>();
        assert_eq!(
            positions.len(),
            expected.len(),
            "every non-Phi instruction exactly once"
        );
        assert_eq!(positions.iter().copied().collect::<BTreeSet<_>>(), expected);
    }
    collecting.functions
}

fn rejected(module: &Module, expected: &str) {
    let collecting = validate_impl::<true>(module).expect_err("specified invalid Module");
    let validation_only = validate_impl::<false>(module).expect_err("same invalid Module");
    assert_eq!(collecting, expected, "collecting first error");
    assert_eq!(validation_only, expected, "validation-only first error");
}

fn before(schedule: &[(usize, usize)], definition: (usize, usize), use_: (usize, usize)) {
    let position = |needle| {
        schedule
            .iter()
            .position(|&point| point == needle)
            .expect("scheduled instruction")
    };
    assert!(
        position(definition) < position(use_),
        "{definition:?} must precede {use_:?}"
    );
}

fn diamond() -> Module {
    module(vec![function_item(
        "diamond",
        vec![
            block(
                "entry",
                vec![inst(
                    None,
                    Op::CondBranch {
                        condition: Val::Bool(true),
                        yes: "left".into(),
                        no: "right".into(),
                    },
                )],
            ),
            block(
                "left",
                vec![add("v", Val::Int(3), Val::Int(4)), branch("join")],
            ),
            block("right", vec![branch("join")]),
            block("join", vec![ret(Val::Int(0))]),
        ],
    )])
}

fn loop_module(reverse: bool) -> Module {
    let entry = block("entry", vec![branch("loop")]);
    let loop_ = block(
        "loop",
        vec![
            inst(
                Some("p"),
                Op::Phi {
                    ty: Ty::Int(32),
                    incoming: vec![
                        (Val::Int(0), "entry".into()),
                        (Val::Local("next".into()), "loop".into()),
                    ],
                },
            ),
            add("next", Val::Local("p".into()), Val::Int(1)),
            inst(
                Some("again"),
                Op::Compare {
                    predicate: "ult".into(),
                    ty: Ty::Int(32),
                    left: Val::Local("next".into()),
                    right: Val::Int(4),
                },
            ),
            inst(
                None,
                Op::CondBranch {
                    condition: Val::Local("again".into()),
                    yes: "loop".into(),
                    no: "exit".into(),
                },
            ),
        ],
    );
    let exit = block("exit", vec![ret(Val::Local("next".into()))]);
    module(vec![function_item(
        "loopfn",
        if reverse {
            vec![entry, exit, loop_]
        } else {
            vec![entry, loop_, exit]
        },
    )])
}

fn duplicate_phi(values: &[i128]) -> Module {
    module(vec![function_item(
        "dup",
        vec![
            block(
                "entry",
                vec![inst(
                    None,
                    Op::CondBranch {
                        condition: Val::Bool(true),
                        yes: "join".into(),
                        no: "join".into(),
                    },
                )],
            ),
            block(
                "join",
                vec![
                    inst(
                        Some("x"),
                        Op::Phi {
                            ty: Ty::Int(32),
                            incoming: values
                                .iter()
                                .map(|&value| (Val::Int(value), "entry".into()))
                                .collect(),
                        },
                    ),
                    ret(Val::Local("x".into())),
                ],
            ),
        ],
    )])
}

fn dead_dependencies(cycle: bool) -> Module {
    module(vec![function_item(
        "dead",
        vec![
            block("entry", vec![ret(Val::Int(0))]),
            block(
                "a",
                vec![
                    add("x", Val::Local("y".into()), Val::Int(1)),
                    ret(Val::Local("x".into())),
                ],
            ),
            block(
                "b",
                vec![
                    add(
                        "y",
                        if cycle {
                            Val::Local("x".into())
                        } else {
                            Val::Int(2)
                        },
                        Val::Int(3),
                    ),
                    ret(Val::Local("y".into())),
                ],
            ),
        ],
    )])
}

fn bad_boolean_operand() -> Item {
    function_item(
        "bad",
        vec![block(
            "entry",
            vec![
                add("v", Val::Int(1), Val::Bool(true)),
                ret(Val::Local("v".into())),
            ],
        )],
    )
}

#[test]
fn reachable_reordered_blocks_keep_real_data_and_sequence_dependencies() {
    let module = module(vec![function_item(
        "ordered",
        vec![
            block(
                "entry",
                vec![add("a", Val::Int(1), Val::Int(2)), branch("produce")],
            ),
            block("use", vec![ret(Val::Local("b".into()))]),
            block(
                "produce",
                vec![add("b", Val::Local("a".into()), Val::Int(3)), branch("use")],
            ),
        ],
    )]);
    let schedules = accepted(&module);
    let schedule = &schedules["ordered"];
    before(schedule, (0, 0), (0, 1));
    before(schedule, (0, 0), (2, 0));
    before(schedule, (2, 0), (2, 1));
    before(schedule, (2, 0), (1, 0));
}

#[test]
fn same_block_forward_reference_is_not_accepted_by_schedule_elision() {
    let module = module(vec![function_item(
        "forward",
        vec![block(
            "entry",
            vec![
                add("a", Val::Local("b".into()), Val::Int(1)),
                add("b", Val::Int(2), Val::Int(3)),
                ret(Val::Local("a".into())),
            ],
        )],
    )]);
    rejected(&module, "SSA definition %b does not dominate its use");
}

#[test]
fn branch_local_definition_must_dominate_a_non_phi_join_use() {
    let mut module = diamond();
    body_mut(&mut module)[3].instructions = vec![ret(Val::Local("v".into()))];
    rejected(&module, "SSA definition %v does not dominate its use");
}

#[test]
fn phi_backedge_is_valid_and_phi_is_not_part_of_builder_schedule() {
    for reverse in [false, true] {
        let module = loop_module(reverse);
        let schedules = accepted(&module);
        let schedule = &schedules["loopfn"];
        let (loop_, exit) = if reverse { (2, 1) } else { (1, 2) };
        assert!(
            !schedule.contains(&(loop_, 0)),
            "Phi is precreated separately"
        );
        before(schedule, (loop_, 1), (loop_, 2));
        before(schedule, (loop_, 2), (loop_, 3));
        before(schedule, (loop_, 1), (exit, 0));
    }
}

#[test]
fn reachable_non_phi_recurrence_remains_a_dominance_error() {
    let mut module = loop_module(false);
    body_mut(&mut module)[1].instructions[0] = add("p", Val::Local("next".into()), Val::Int(1));
    rejected(&module, "SSA definition %next does not dominate its use");
}

#[test]
fn duplicate_cfg_edges_accept_equal_phi_values_but_reject_different_values() {
    let schedules = accepted(&duplicate_phi(&[1, 1]));
    assert_eq!(schedules["dup"].len(), 2);
    assert!(!schedules["dup"].contains(&(1, 0)));
    rejected(
        &duplicate_phi(&[1, 2]),
        "duplicate phi edges from a predecessor carry different values",
    );
}

#[test]
fn phi_predecessors_match_the_full_cfg_multiset() {
    for values in [&[1][..], &[1, 1, 1][..]] {
        rejected(
            &duplicate_phi(values),
            "phi incoming edges do not match CFG predecessor multiset",
        );
    }
}

#[test]
fn phi_source_must_dominate_its_named_predecessor() {
    let mut module = diamond();
    body_mut(&mut module)[3].instructions = vec![
        inst(
            Some("p"),
            Op::Phi {
                ty: Ty::Int(32),
                incoming: vec![
                    (Val::Local("v".into()), "left".into()),
                    (Val::Local("v".into()), "right".into()),
                ],
            },
        ),
        ret(Val::Local("p".into())),
    ];
    rejected(&module, "phi source %v does not dominate predecessor");
}

#[test]
fn unreachable_acyclic_uses_still_receive_a_complete_dependency_schedule() {
    let schedules = accepted(&dead_dependencies(false));
    let schedule = &schedules["dead"];
    before(schedule, (2, 0), (1, 0));
    before(schedule, (1, 0), (1, 1));
    before(schedule, (2, 0), (2, 1));
}

#[test]
fn unreachable_non_phi_cycle_keeps_the_original_kahn_rejection() {
    let module = dead_dependencies(true);
    rejected(&module, "cycle in non-phi SSA or instruction sequence");
}

#[test]
fn operation_types_and_cast_direction_are_checked_before_schedule_shortcut() {
    rejected(
        &module(vec![bad_boolean_operand()]),
        "@bad / entry / 0: boolean constant is not i1",
    );
    let cast = module(vec![function_item(
        "cast",
        vec![block(
            "entry",
            vec![
                inst(
                    Some("v"),
                    Op::Cast {
                        opcode: "trunc".into(),
                        arg: Arg::new(Ty::Int(8), Val::Int(1)),
                        to: Ty::Int(32),
                    },
                ),
                ret(Val::Local("v".into())),
            ],
        )],
    )]);
    rejected(&cast, "@cast / entry / 0: invalid machine cast");
}

#[test]
fn first_error_follows_original_function_order_in_mixed_reachability_modules() {
    let mut cycle_first = dead_dependencies(true);
    cycle_first.items.push(bad_boolean_operand());
    rejected(&cycle_first, "cycle in non-phi SSA or instruction sequence");
    cycle_first.items.swap(0, 1);
    rejected(&cycle_first, "@bad / entry / 0: boolean constant is not i1");
}
