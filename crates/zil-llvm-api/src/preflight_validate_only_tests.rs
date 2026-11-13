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
        blocks: Some((blocks).into()),
    })
}

fn module(items: Vec<Item>) -> Module {
    Module {
        triple: "x86_64-unknown-linux-gnu".into(),
        fingerprint: "validate-only-ci".into(),
        items: items.into(),
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

#[test]
fn borrowed_result_types_preserve_aggregate_projection_and_gep() {
    let aggregate = Ty::Named("Pair".into());
    let array = Ty::array(2, Ty::Int(32));
    let mut function = function_item(
        "projection",
        vec![block(
            "entry",
            vec![
                inst(
                    Some("pair"),
                    Op::Freeze(Arg::new(aggregate.clone(), Val::Local("input".into()))),
                ),
                inst(
                    Some("array"),
                    Op::Extract {
                        arg: Arg::new(aggregate.clone(), Val::Local("pair".into())),
                        index: 0,
                    },
                ),
                inst(
                    Some("value"),
                    Op::Extract {
                        arg: Arg::new(array.clone(), Val::Local("array".into())),
                        index: 1,
                    },
                ),
                inst(
                    Some("slot"),
                    Op::Alloca {
                        ty: aggregate.clone(),
                        align: 8,
                    },
                ),
                inst(
                    Some("field"),
                    Op::Gep {
                        ty: aggregate,
                        ptr: Val::Local("slot".into()),
                        indices: vec![
                            Arg::new(Ty::Int(64), Val::Int(0)),
                            Arg::new(Ty::Int(32), Val::Int(0)),
                            Arg::new(Ty::Int(64), Val::Int(1)),
                        ],
                    },
                ),
                inst(
                    None,
                    Op::Store {
                        arg: Arg::new(Ty::Int(32), Val::Local("value".into())),
                        ptr: Val::Local("field".into()),
                        align: 4,
                    },
                ),
                inst(
                    Some("loaded"),
                    Op::Load {
                        ty: Ty::Int(32),
                        ptr: Val::Local("field".into()),
                        align: 4,
                    },
                ),
                ret(Val::Local("loaded".into())),
            ],
        )],
    );
    let Item::Function(f) = &mut function else {
        unreachable!()
    };
    f.signature.params.push(Param {
        ty: Ty::Named("Pair".into()),
        name: Some("input".into()),
        attrs: vec![],
    });
    let mut model = module(vec![Item::NamedType("Pair".into(), vec![array]), function]);
    accepted(&model);

    // A named projection borrows the same field stored in this current model;
    // ordinary result types borrow the instruction rather than a copied tree.
    let Item::NamedType(name, fields) = &model.items[0] else {
        unreachable!()
    };
    let world = World {
        named: BTreeMap::from([(name.as_str(), fields)]),
        symbols: BTreeMap::new(),
        valid_named: RefCell::new(BTreeMap::new()),
    };
    let Item::Function(f) = &model.items[1] else {
        unreachable!()
    };
    let instructions = &f.blocks.as_ref().unwrap()[0].instructions;
    assert!(std::ptr::eq(
        world.result(&instructions[1].op).unwrap().unwrap(),
        &fields[0]
    ));
    let Op::Freeze(arg) = &instructions[0].op else {
        unreachable!()
    };
    assert!(std::ptr::eq(
        world.result(&instructions[0].op).unwrap().unwrap(),
        &arg.ty
    ));

    let Item::Function(f) = &mut model.items[1] else {
        unreachable!()
    };
    let Op::Gep { indices, .. } = &mut f.blocks.as_mut().unwrap()[0].instructions[4].op else {
        unreachable!()
    };
    indices[1].ty = Ty::Int(64);
    rejected(
        &model,
        "@projection / entry / 4: struct GEP index must be i32",
    );
}

#[test]
fn borrowed_results_keep_result_type_and_identifier_error_order() {
    for (operation, expected) in [
        (
            Op::Freeze(Arg::new(Ty::Int(7), Val::Zero)),
            "unsupported machine integer width",
        ),
        (
            Op::Extract {
                arg: Arg::new(Ty::Struct(vec![Ty::Int(7)]), Val::Zero),
                index: 1,
            },
            "aggregate index outside struct",
        ),
        (
            Op::Extract {
                arg: Arg::new(Ty::Struct(vec![Ty::Int(7)]), Val::Zero),
                index: 0,
            },
            "unsupported machine integer width",
        ),
        (
            Op::Extract {
                arg: Arg::new(Ty::array(2, Ty::Int(32)), Val::Zero),
                index: 2,
            },
            "aggregate index outside machine array",
        ),
        (
            Op::Freeze(Arg::new(Ty::Struct(vec![Ty::Int(32)]), Val::Zero)),
            "invalid/duplicate SSA definition %!invalid",
        ),
    ] {
        rejected(
            &module(vec![function_item(
                "ordered",
                vec![block(
                    "entry",
                    vec![inst(Some("!invalid"), operation), ret(Val::Int(0))],
                )],
            )]),
            expected,
        );
    }
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

fn scratch_world(module: &Module) -> World<'_> {
    World {
        named: BTreeMap::new(),
        symbols: module
            .items
            .iter()
            .map(|item| {
                let Item::Function(f) = item else {
                    panic!("function fixture")
                };
                (f.name.as_str(), item)
            })
            .collect(),
        valid_named: RefCell::new(BTreeMap::new()),
    }
}

#[test]
fn reused_scratch_preserves_errors_and_schedules_across_function_boundaries() {
    fn check<const COLLECT: bool>() {
        let model = module(vec![
            function_item(
                "producer",
                vec![block(
                    "entry",
                    vec![
                        add("x", Val::Int(1), Val::Int(2)),
                        ret(Val::Local("x".into())),
                    ],
                )],
            ),
            loop_module(false).items.remove(0),
            function_item(
                "missing",
                vec![block("entry", vec![ret(Val::Local("x".into()))])],
            ),
            function_item("dangling", vec![block("entry", vec![branch("loop")])]),
            diamond().items.remove(0),
            dead_dependencies(false).items.remove(0),
        ]);
        let world = scratch_world(&model);
        let mut reused = FunctionScratch::default();
        for id in [0, 1, 2, 1, 3, 4, 5, 0] {
            let Item::Function(f) = &model.items[id] else {
                unreachable!()
            };
            let expected = function_in::<COLLECT>(&world, f, &mut FunctionScratch::default());
            if id == 2 {
                assert_eq!(
                    expected.as_ref().unwrap_err(),
                    "@missing / entry / 0: undefined SSA value %x"
                );
                // The retention cap must also apply when the next attempt fails.
                reused.uses.reserve(1024);
            } else if id == 3 {
                assert_eq!(expected.as_ref().unwrap_err(), "unknown block %loop");
            }
            assert_eq!(function::<COLLECT>(&world, f, &mut reused), expected);
            assert!(reused.uses.capacity() <= 256);
        }
    }
    check::<false>();
    check::<true>();
}

#[test]
fn large_functions_use_temporary_scratch_without_growing_the_small_pool() {
    let many_blocks = (0..33)
        .map(|id| {
            block(
                &format!("b{id}"),
                vec![if id == 32 {
                    ret(Val::Int(0))
                } else {
                    branch(&format!("b{}", id + 1))
                }],
            )
        })
        .collect();
    let mut many_instructions = (0..256)
        .map(|id| add(&format!("v{id}"), Val::Int(1), Val::Int(2)))
        .collect::<Vec<_>>();
    many_instructions.push(ret(Val::Local("undefined".into())));
    let model = module(vec![
        function_item("small", vec![block("entry", vec![ret(Val::Int(0))])]),
        function_item("many_blocks", many_blocks),
        function_item("many_instructions", vec![block("entry", many_instructions)]),
    ]);
    let world = scratch_world(&model);
    let mut reused = FunctionScratch::default();
    let Item::Function(small) = &model.items[0] else {
        unreachable!()
    };
    function::<false>(&world, small, &mut reused).unwrap();
    let capacities = |s: &FunctionScratch<'_>| {
        [
            s.labels.capacity(),
            s.defs.capacity(),
            s.succ.len(),
            s.pred.len(),
            s.dominators.seen.capacity(),
            s.dominators.post.capacity(),
            s.dominators.stack.capacity(),
            s.dominators.rank.capacity(),
            s.dominators.idom.capacity(),
        ]
    };
    let before = capacities(&reused);
    for item in &model.items[1..] {
        let Item::Function(f) = item else {
            unreachable!()
        };
        assert!(!reusable_function_scratch(f));
        let expected = function_in::<false>(&world, f, &mut FunctionScratch::default());
        assert_eq!(function::<false>(&world, f, &mut reused), expected);
        assert_eq!(capacities(&reused), before);
        function::<false>(&world, small, &mut reused).unwrap();
    }
    reused.uses.reserve(1024);
    function::<false>(&world, small, &mut reused).unwrap();
    assert!(reused.uses.capacity() <= 256);
}

#[test]
fn noinline_is_a_function_boundary_not_a_result_or_parameter_attribute() {
    let mut source = module(vec![function_item(
        "copy_boundary",
        vec![block("entry", vec![ret(Val::Int(0))])],
    )]);
    let Item::Function(function) = &mut source.items[0] else {
        panic!()
    };
    function.attrs.push(Attr::NoInline);
    accepted(&source);
    assert!(source.to_text().contains("noinline"));
    let Item::Function(function) = &mut source.items[0] else {
        panic!()
    };
    function.signature.result_attrs.push(Attr::NoInline);
    assert!(validate_impl::<true>(&source).is_err());
    assert!(validate_impl::<false>(&source).is_err());
}

fn atomic_module() -> Module {
    let mut atomic = function_item(
        "atomic_increment",
        vec![block(
            "entry",
            vec![
                inst(
                    Some("old"),
                    Op::AtomicRmw {
                        operation: AtomicRmwOp::Add,
                        arg: Arg::new(Ty::Int(32), 1),
                        ptr: Val::Local("cell".into()),
                        align: 4,
                        ordering: AtomicOrdering::SeqCst,
                    },
                ),
                ret(Val::Local("old".into())),
            ],
        )],
    );
    let Item::Function(function) = &mut atomic else {
        unreachable!()
    };
    function.signature.params.push(Param::new(Ty::Ptr, "cell"));
    let mut result = module(vec![atomic]);
    for (name, store) in [("atomic_set", true), ("atomic_get", false)] {
        let operation = if store {
            inst(
                None,
                Op::AtomicStore {
                    arg: Arg::new(Ty::Int(32), 7),
                    ptr: Val::Local("cell".into()),
                    align: 4,
                    ordering: AtomicOrdering::Release,
                },
            )
        } else {
            inst(
                Some("loaded"),
                Op::AtomicLoad {
                    ty: Ty::Int(32),
                    ptr: Val::Local("cell".into()),
                    align: 4,
                    ordering: AtomicOrdering::Acquire,
                },
            )
        };
        let mut item = function_item(
            name,
            vec![block(
                "entry",
                vec![
                    operation,
                    ret(if store {
                        Val::Int(7)
                    } else {
                        Val::Local("loaded".into())
                    }),
                ],
            )],
        );
        let Item::Function(function) = &mut item else {
            unreachable!()
        };
        function.signature.params.push(Param::new(Ty::Ptr, "cell"));
        result.items.push(item);
    }
    result
}

#[test]
fn actual_atomics_have_typed_operands_and_valid_memory_orders() {
    let source = atomic_module();
    accepted(&source);
    let text = source.to_text();
    assert!(text.contains("atomicrmw add ptr %cell, i32 1 seq_cst, align 4"));
    assert!(text.contains("store atomic i32 7, ptr %cell release, align 4"));
    assert!(text.contains("load atomic i32, ptr %cell acquire, align 4"));
    for (op, expected) in [
        (
            Op::AtomicLoad {
                ty: Ty::Int(32),
                ptr: Val::Local("cell".into()),
                align: 4,
                ordering: AtomicOrdering::Release,
            },
            "atomic load cannot have release ordering",
        ),
        (
            Op::AtomicStore {
                arg: Arg::new(Ty::Int(32), 1),
                ptr: Val::Local("cell".into()),
                align: 4,
                ordering: AtomicOrdering::Acquire,
            },
            "atomic store cannot have acquire ordering",
        ),
        (
            Op::AtomicLoad {
                ty: Ty::Int(32),
                ptr: Val::Local("cell".into()),
                align: 0,
                ordering: AtomicOrdering::Relaxed,
            },
            "atomic operand requires explicit natural alignment",
        ),
        (
            Op::AtomicRmw {
                operation: AtomicRmwOp::Add,
                arg: Arg::new(Ty::Int(32), 1),
                ptr: Val::Local("cell".into()),
                align: 2,
                ordering: AtomicOrdering::AcqRel,
            },
            "atomic operand requires explicit natural alignment",
        ),
    ] {
        let mut invalid = source.clone();
        let instruction = &mut body_mut(&mut invalid)[0].instructions[0];
        instruction.result = (!matches!(op, Op::AtomicStore { .. })).then(|| "old".into());
        instruction.op = op;
        if instruction.result.is_none() {
            body_mut(&mut invalid)[0].instructions[1] = ret(Val::Int(0));
        }
        rejected(
            &invalid,
            &format!("@atomic_increment / entry / 0: {expected}"),
        );
    }
}

#[cfg(feature = "native")]
#[test]
fn native_atomics_synchronize_real_threads_at_o0_and_o2() {
    use std::{
        fs,
        process::Command,
        sync::atomic::{AtomicU64, Ordering},
    };
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let scratch = std::env::temp_dir().join(format!(
        "zil-preflight-validate-only-tests-atomic-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&scratch).unwrap();
    let source = atomic_module();
    accepted(&source);
    fs::write(scratch.join("atomic.ll"), source.to_text()).unwrap();
    // Machine-adapter harness only: source-language acceptance is tested by the
    // driver separately. Every concurrent access reaches the generated RMW.
    fs::write(
        scratch.join("host.c"),
        r#"
#include <stdint.h>
#include <pthread.h>
extern int32_t atomic_increment(int32_t *);
extern int32_t atomic_set(int32_t *);
extern int32_t atomic_get(int32_t *);
static int32_t value;
static void *worker(void *unused) {
    (void)unused;
    for (int i=0; i<25000; ++i) atomic_increment(&value);
    return 0;
}
int main(void) {
    pthread_t threads[4];
    atomic_set(&value);
    for (int i=0; i<4; ++i) if (pthread_create(&threads[i],0,worker,0)) return 2;
    for (int i=0; i<4; ++i) if (pthread_join(threads[i],0)) return 3;
    return atomic_get(&value) == 100007 ? 0 : 4;
}
"#,
    )
    .unwrap();
    for (opt, flag) in [
        (crate::Optimization::O0, "-O0"),
        (crate::Optimization::O2, "-O2"),
    ] {
        let object = crate::compile_module(
            &source,
            &crate::Config {
                triple: "x86_64-unknown-linux-gnu",
                optimization: opt,
            },
            &|| false,
        )
        .unwrap();
        let object_path = scratch.join(format!("api{flag}.o"));
        fs::write(&object_path, object.bytes).unwrap();
        for (name, input) in [("api", object_path), ("text", scratch.join("atomic.ll"))] {
            let executable = scratch.join(format!("{name}{flag}"));
            let output = Command::new("clang")
                .arg(flag)
                .arg(&input)
                .arg(scratch.join("host.c"))
                .arg("-pthread")
                .arg("-o")
                .arg(&executable)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let status = Command::new(&executable).status().unwrap();
            assert!(status.success(), "{name} {flag}: {status}");
        }
    }
    fs::remove_dir_all(scratch).unwrap();
}

#[test]
fn ieee_float_bits_and_operations_have_distinct_machine_types() {
    let make = |ty: Ty, left: Val, right: Val| {
        module(vec![function_item(
            "float_probe",
            vec![block(
                "entry",
                vec![
                    inst(
                        Some("sum"),
                        Op::Binary {
                            opcode: "fadd".into(),
                            ty: ty.clone(),
                            left,
                            right,
                        },
                    ),
                    inst(
                        Some("comparison"),
                        Op::Compare {
                            predicate: "oeq".into(),
                            ty,
                            left: Val::Local("sum".into()),
                            right: Val::F32(3.0f32.to_bits()),
                        },
                    ),
                    inst(
                        Some("result"),
                        Op::Cast {
                            opcode: "zext".into(),
                            arg: Arg::new(Ty::Int(1), Val::Local("comparison".into())),
                            to: Ty::Int(32),
                        },
                    ),
                    ret(Val::Local("result".into())),
                ],
            )],
        )])
    };
    let valid = make(
        Ty::F32,
        Val::F32(1.0f32.to_bits()),
        Val::F32(2.0f32.to_bits()),
    );
    accepted(&valid);
    let rendered = valid.to_text();
    assert!(rendered.contains("fadd float bitcast (i32 1065353216 to float)"));
    assert!(rendered.contains("fcmp oeq float"));
    for changed in 0..6 {
        let mut damaged = valid.clone();
        let instructions = &mut body_mut(&mut damaged)[0].instructions;
        match changed {
            0 => {
                if let Op::Binary { opcode, .. } = &mut instructions[0].op {
                    *opcode = "add".into();
                }
            }
            1 => {
                if let Op::Binary { left, .. } = &mut instructions[0].op {
                    *left = Val::F64(1.0f64.to_bits());
                }
            }
            2 => {
                if let Op::Binary { right, .. } = &mut instructions[0].op {
                    *right = Val::Int(2);
                }
            }
            3 => {
                if let Op::Compare { predicate, .. } = &mut instructions[1].op {
                    *predicate = "eq".into();
                }
            }
            4 => {
                if let Op::Binary { opcode, .. } = &mut instructions[0].op {
                    *opcode = "fadd fast".into();
                }
            }
            _ => {
                if let Op::Compare { predicate, .. } = &mut instructions[1].op {
                    *predicate = "oeq fast".into();
                }
            }
        }
        assert!(validate_impl::<true>(&damaged).is_err());
        assert!(validate_impl::<false>(&damaged).is_err());
    }
}

fn compare_exchange_module(success: AtomicOrdering, failure: AtomicOrdering) -> Module {
    let answer = Ty::Struct(vec![Ty::Int(32), Ty::Int(1)]);
    module(vec![Item::Function(Function {
        name: "atomic_compare_exchange".into(),
        linkage: Linkage::External,
        signature: Signature::new(
            Ty::Int(64),
            vec![
                Param::new(Ty::Ptr, "cell"),
                Param::new(Ty::Int(32), "expected"),
                Param::new(Ty::Int(32), "desired"),
            ],
        ),
        attrs: vec![],
        blocks: Some(
            (vec![block(
                "entry",
                vec![
                    inst(
                        Some("answer"),
                        Op::AtomicCompareExchange {
                            result: answer.clone(),
                            expected: Arg::new(Ty::Int(32), Val::Local("expected".into())),
                            desired: Val::Local("desired".into()),
                            ptr: Val::Local("cell".into()),
                            align: 4,
                            success,
                            failure,
                        },
                    ),
                    inst(
                        Some("old"),
                        Op::Extract {
                            arg: Arg::new(answer.clone(), Val::Local("answer".into())),
                            index: 0,
                        },
                    ),
                    inst(
                        Some("success"),
                        Op::Extract {
                            arg: Arg::new(answer, Val::Local("answer".into())),
                            index: 1,
                        },
                    ),
                    inst(
                        Some("old.bits"),
                        Op::Cast {
                            opcode: "zext".into(),
                            arg: Arg::new(Ty::Int(32), Val::Local("old".into())),
                            to: Ty::Int(64),
                        },
                    ),
                    inst(
                        Some("success.wide"),
                        Op::Cast {
                            opcode: "zext".into(),
                            arg: Arg::new(Ty::Int(1), Val::Local("success".into())),
                            to: Ty::Int(64),
                        },
                    ),
                    inst(
                        Some("success.bits"),
                        Op::Binary {
                            opcode: "shl".into(),
                            ty: Ty::Int(64),
                            left: Val::Local("success.wide".into()),
                            right: Val::Int(32),
                        },
                    ),
                    inst(
                        Some("packed"),
                        Op::Binary {
                            opcode: "or".into(),
                            ty: Ty::Int(64),
                            left: Val::Local("success.bits".into()),
                            right: Val::Local("old.bits".into()),
                        },
                    ),
                    inst(
                        None,
                        Op::Return(Some(Arg::new(Ty::Int(64), Val::Local("packed".into())))),
                    ),
                ],
            )])
            .into(),
        ),
    })])
}

#[test]
fn strong_compare_exchange_has_exact_result_and_c11_order_matrix() {
    use AtomicOrdering::{AcqRel, Acquire, Relaxed, Release, SeqCst};
    let orders = [Relaxed, Acquire, Release, AcqRel, SeqCst];
    let admitted = [
        [true, false, false, false, false],
        [true, true, false, false, false],
        [true, false, false, false, false],
        [true, true, false, false, false],
        [true, true, false, false, true],
    ];
    for (i, &success) in orders.iter().enumerate() {
        for (j, &failure) in orders.iter().enumerate() {
            let source = compare_exchange_module(success, failure);
            if admitted[i][j] {
                accepted(&source);
            } else {
                rejected(
                    &source,
                    "@atomic_compare_exchange / entry / 0: compare exchange success/failure ordering pair is outside the C11 subset",
                );
                #[cfg(feature = "native")]
                assert_eq!(
                    crate::compile_module(
                        &source,
                        &crate::Config {
                            triple: "x86_64-unknown-linux-gnu",
                            optimization: crate::Optimization::O0
                        },
                        &|| false
                    )
                    .unwrap_err()
                    .kind,
                    crate::ErrorKind::Verify
                );
            }
        }
    }
    let source = compare_exchange_module(AcqRel, Acquire);
    let text = source.to_text();
    assert!(
        text.contains("cmpxchg ptr %cell, i32 %expected, i32 %desired acq_rel acquire, align 4")
    );
    assert!(!text.contains("cmpxchg weak"));
    for case in 0..5 {
        let mut bad = source.clone();
        let Op::AtomicCompareExchange {
            result,
            expected,
            desired,
            ptr,
            align,
            ..
        } = &mut body_mut(&mut bad)[0].instructions[0].op
        else {
            unreachable!()
        };
        match case {
            0 => *result = Ty::Int(32),
            1 => *desired = Val::Bool(true),
            2 => *ptr = Val::Int(1),
            3 => *align = 2,
            4 => expected.attrs.push(Attr::ZeroExt),
            _ => unreachable!(),
        }
        assert!(validate_impl::<true>(&bad).is_err(), "case={case}");
        assert!(validate_impl::<false>(&bad).is_err(), "case={case}");
    }
}

#[cfg(feature = "native")]
#[test]
fn strong_compare_exchange_native_observes_failure_and_real_contention() {
    use AtomicOrdering::{AcqRel, Acquire, Relaxed, Release, SeqCst};
    use std::{fs, process::Command};
    let pairs = [
        (Relaxed, Relaxed),
        (Acquire, Relaxed),
        (Acquire, Acquire),
        (Release, Relaxed),
        (AcqRel, Relaxed),
        (AcqRel, Acquire),
        (SeqCst, Relaxed),
        (SeqCst, Acquire),
        (SeqCst, SeqCst),
    ];
    let mut source = module(vec![]);
    let mut declarations = String::new();
    let mut names = vec![];
    for (i, (success, failure)) in pairs.into_iter().enumerate() {
        let mut part = compare_exchange_module(success, failure);
        let Item::Function(function) = &mut part.items[0] else {
            unreachable!()
        };
        function.name = format!("cas_{i}");
        declarations.push_str(&format!(
            "extern uint64_t cas_{i}(int32_t*,int32_t,int32_t);\n"
        ));
        names.push(format!("cas_{i}"));
        source.items.extend(part.items);
    }
    accepted(&source);
    assert_eq!(
        crate::compile_module(
            &source,
            &crate::Config {
                triple: "x86_64-unknown-linux-gnu",
                optimization: crate::Optimization::O0
            },
            &|| true
        )
        .unwrap_err()
        .kind,
        crate::ErrorKind::Cancelled
    );
    let scratch = std::env::temp_dir().join(format!("zil-strong-cas-{}", std::process::id()));
    fs::create_dir_all(&scratch).unwrap();
    fs::write(scratch.join("cas.ll"), source.to_text()).unwrap();
    let host = format!("#include <stdint.h>\n#include <pthread.h>\n{declarations}\n")
        + r#"
typedef uint64_t (*cas_fn)(int32_t*,int32_t,int32_t);
static int32_t value;
static cas_fn active;
static void *worker(void *unused) {
    (void)unused;
    for(int i=0;i<5000;++i) {
        int32_t expected=0;
        for(;;) {
            uint64_t answer=active(&value,expected,expected+1);
            if((uint32_t)(answer>>32)==1) break;
            expected=(int32_t)(uint32_t)answer;
        }
    }
    return 0;
}
int main(void) {
    cas_fn functions[]={"#
        + &names.join(",")
        + r#"};
    for(unsigned i=0;i<sizeof(functions)/sizeof(functions[0]);++i) {
        active=functions[i];int32_t local=-7;
        uint64_t yes=active(&local,-7,9);
        if((int32_t)(uint32_t)yes!=-7 || (uint32_t)(yes>>32)!=1 || local!=9) return 1;
        uint64_t no=active(&local,-7,99);
        if((int32_t)(uint32_t)no!=9 || (uint32_t)(no>>32)!=0 || local!=9) return 2;
        value=0;pthread_t threads[4];
        for(int j=0;j<4;++j) if(pthread_create(&threads[j],0,worker,0)) return 3;
        for(int j=0;j<4;++j) if(pthread_join(threads[j],0)) return 4;
        if(value!=20000) return 5;
    }
    return 0;
}
"#;
    fs::write(scratch.join("host.c"), host).unwrap();
    for (optimization, flag) in [
        (crate::Optimization::O0, "-O0"),
        (crate::Optimization::O2, "-O2"),
    ] {
        let object = crate::compile_module(
            &source,
            &crate::Config {
                triple: "x86_64-unknown-linux-gnu",
                optimization,
            },
            &|| false,
        )
        .unwrap();
        assert_eq!(object.receipt.parse_ns, 0);
        assert!(object.receipt.verify_before_after);
        let object_path = scratch.join(format!("api{flag}.o"));
        fs::write(&object_path, object.bytes).unwrap();
        for (backend, input) in [("api", object_path), ("text", scratch.join("cas.ll"))] {
            let executable = scratch.join(format!("{backend}{flag}"));
            let linked = Command::new("clang")
                .arg(flag)
                .arg(input)
                .arg(scratch.join("host.c"))
                .args(["-pthread", "-o"])
                .arg(&executable)
                .output()
                .unwrap();
            assert!(
                linked.status.success(),
                "{}",
                String::from_utf8_lossy(&linked.stderr)
            );
            let actual = Command::new(&executable).output().unwrap();
            assert_eq!(
                actual.status.code(),
                Some(0),
                "{backend}/{flag}: {actual:?}"
            );
        }
    }
    fs::remove_dir_all(scratch).unwrap();
}

fn selected_integers() -> Module {
    let mut item = function_item("select_integer", vec![block("entry", vec![
        inst(Some("chosen"), Op::Select { ty: Ty::Int(32), condition: Val::Local("condition".into()), yes: Val::Local("yes".into()), no: Val::Local("no".into()) }),
        ret(Val::Local("chosen".into())),
    ])]);
    let Item::Function(f) = &mut item else { unreachable!() };
    f.signature.params = vec![Param::new(Ty::Int(1), "condition"),Param::new(Ty::Int(32), "yes"),Param::new(Ty::Int(32), "no")];
    module(vec![item])
}

#[test]
fn integer_select_binds_both_arms_condition_types_and_dominance() {
    let valid = selected_integers();
    accepted(&valid);
    let (receipt, reused) = crate::validate_module_with_receipt(&valid,None).unwrap();
    assert!(!reused);
    for operand in 0..3 {
        let mut changed = valid.clone();
        let Op::Select { condition, yes, no, .. } = &mut body_mut(&mut changed)[0].instructions[0].op else { unreachable!() };
        *match operand { 0=>condition,1=>yes,_=>no } = if operand == 0 { Val::Bool(false) } else { Val::Int(73) };
        assert!(!crate::validate_module_with_receipt(&changed,Some(&receipt)).unwrap().1);
    }
    let mut invalid = valid.clone();
    let Op::Select { condition, .. } = &mut body_mut(&mut invalid)[0].instructions[0].op else { unreachable!() };
    *condition = Val::Local("yes".into());
    assert!(validate_impl::<true>(&invalid).is_err());
    assert!(validate_impl::<false>(&invalid).is_err());
    // The third operand must enter the actual schedule/dominance walk. Merely
    // checking its spelling/type would allow a value defined only on one arm.
    let mut invalid = valid.clone();
    *body_mut(&mut invalid) = vec![
        block("entry", vec![inst(None,Op::CondBranch {condition: Val::Local("condition".into()),yes:"left".into(),no:"right".into()})]),
        block("left", vec![add("left_only",Val::Int(1),Val::Int(2)),branch("join")]),
        block("right", vec![branch("join")]),
        block("join",vec![inst(Some("chosen"),Op::Select {ty:Ty::Int(32),condition:Val::Local("condition".into()),yes:Val::Local("yes".into()),no:Val::Local("left_only".into())}),ret(Val::Local("chosen".into()))]),
    ];
    let collecting = validate_impl::<true>(&invalid).unwrap_err();
    assert!(collecting.contains("dominat"),"{collecting}");
    assert_eq!(validate_impl::<false>(&invalid).unwrap_err(),collecting);
    assert!(crate::validate_module_with_receipt(&invalid,Some(&receipt)).is_err());
}

#[cfg(all(feature = "native", target_os = "linux", target_arch = "x86_64"))]
#[test]
fn integer_select_native_and_text_paths_observe_only_the_chosen_arm() {
    use std::{fs, process::Command,sync::atomic::{AtomicU64,Ordering}};
    static NEXT:AtomicU64=AtomicU64::new(0);
    let scratch=std::env::temp_dir().join(format!("zil-select-native-{}-{}",std::process::id(),NEXT.fetch_add(1,Ordering::Relaxed)));
    fs::create_dir(&scratch).unwrap();
    let mut source=selected_integers();
    source.items.push(function_item("unselected_poison",vec![block("entry",vec![
        inst(Some("v"),Op::Select {ty:Ty::Int(32),condition:Val::Bool(true),yes:Val::Int(37),no:Val::Poison}),ret(Val::Local("v".into())),
    ])]));
    source.items.push(function_item("unselected_undef",vec![block("entry",vec![
        inst(Some("v"),Op::Select {ty:Ty::Int(32),condition:Val::Bool(false),yes:Val::Undef,no:Val::Int(23)}),ret(Val::Local("v".into())),
    ])]));
    accepted(&source);
    fs::write(scratch.join("select.ll"),source.to_text()).unwrap();
    // C is an ABI harness for the independently preflighted machine module;
    // real source-language cleanup coverage is in the driver/backend suite.
    fs::write(scratch.join("host.c"),r#"
#include <stdint.h>
extern int32_t select_integer(_Bool,int32_t,int32_t);
extern int32_t unselected_poison(void);
extern int32_t unselected_undef(void);
int main(void) {
    int32_t values[]={INT32_MIN,-1,0,1,INT32_MAX};
    for(int c=0;c!=2;++c) for(int a=0;a!=5;++a) for(int b=0;b!=5;++b)
      if(select_integer(c,values[a],values[b])!=(c?values[a]:values[b])) return 1;
    if(unselected_poison()!=37 || unselected_undef()!=23) return 2;
    return 0;
}
"#).unwrap();
    for (optimization,flag) in [(crate::Optimization::O0,"-O0"),(crate::Optimization::O2,"-O2")] {
        let object=crate::compile_module(&source,&crate::Config {triple:"x86_64-unknown-linux-gnu",optimization},&||false).unwrap();
        assert!(object.report.verify_before_after);
        let api=scratch.join(format!("api{flag}.o"));fs::write(&api,object.bytes).unwrap();
        for (kind,input) in [("api",api),("text",scratch.join("select.ll"))] {
            let output=scratch.join(format!("{kind}{flag}"));
            let linked=Command::new(std::env::var_os("ZIL_CLANG").unwrap_or_else(||"clang".into())).arg(flag).arg(input).arg(scratch.join("host.c")).arg("-o").arg(&output).output().unwrap();
            assert!(linked.status.success(),"{}",String::from_utf8_lossy(&linked.stderr));
            assert!(Command::new(output).status().unwrap().success());
        }
    }
    fs::remove_dir_all(scratch).unwrap();
}
