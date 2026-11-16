//! Bounded machine-model regressions; source/native and formal quality remain separate.
use super::*;
use zil_llvm_api::model::{Attr, Linkage, Param, Signature};

#[test]
fn restart_shared_bodies_plan_once_and_keep_current_headers() {
    let mut module = tiny(8, true, &[1, 2, 3]);
    let original = function(&mut module).clone();
    for index in 0..64 {
        let mut copy = original.clone();
        copy.name = format!("copy{index}");
        module.items.push(Item::Function(copy));
    }
    // A valid distinct header must not borrow the conservative full-input
    // result, even when its executable blocks still have the same owner.
    let mut distinct = original.clone();
    distinct.name = "distinct_header".into();
    distinct.linkage = Linkage::Internal;
    module.items.push(Item::Function(distinct));
    let mut expected = module.clone();
    let expected_removed = fold_checked_add_chains(&mut expected);
    let mut cache = Cache::default();
    assert_eq!(
        fold_checked_add_chains_cached(&mut module, &mut cache),
        expected_removed
    );
    assert_eq!(module, expected);
    assert_eq!((cache.checked_functions, cache.reused_functions), (2, 64));
    let first = match &module.items[2] {
        Item::Function(f) => f,
        _ => unreachable!(),
    };
    for item in &module.items[3..67] {
        let Item::Function(copy) = item else {
            unreachable!()
        };
        assert!(
            first
                .blocks
                .as_ref()
                .unwrap()
                .ptr_eq(copy.blocks.as_ref().unwrap())
        );
        assert_ne!(first.name, copy.name);
    }
}

#[test]
fn restart_shared_chain_does_not_reuse_changed_body_or_bad_current_context() {
    let mut module = tiny(8, true, &[1, 2]);
    let original = function(&mut module).clone();
    let mut changed = original.clone();
    changed.name = "changed_body".into();
    if let Op::Call { args, .. } = &mut changed.blocks.as_mut().unwrap()[1].instructions[0].op {
        args[1].value = Val::Int(4);
    } else {
        unreachable!()
    }
    module.items.push(Item::Function(changed));
    let mut expected = module.clone();
    fold_checked_add_chains(&mut expected);
    let mut cache = Cache::default();
    fold_checked_add_chains_cached(&mut module, &mut cache);
    assert_eq!(module, expected);
    assert_eq!((cache.checked_functions, cache.reused_functions), (2, 0));
    let mut invalid = tiny(8, true, &[1, 2]);
    invalid.items.push(Item::Function(original)); // duplicate real symbol
    let before = invalid.clone();
    let error = validate_module(&invalid).unwrap_err();
    assert_eq!(fold_checked_add_chains_cached(&mut invalid, &mut cache), 0);
    assert_eq!(invalid, before);
    assert_eq!(validate_module(&invalid).unwrap_err(), error);
}
use zil_llvm_api::validate_module;

fn tiny(bits: u32, signed: bool, constants: &[i128]) -> Module {
    let ty = Ty::Int(bits);
    let pair = Ty::Struct(vec![ty.clone(), Ty::Int(1)]);
    let intrinsic = format!(
        "llvm.{}add.with.overflow.i{bits}",
        if signed { "s" } else { "u" }
    );
    let declaration = |name, result, params| Function {
        name,
        linkage: Linkage::External,
        signature: Signature::new(result, params),
        attrs: vec![],
        blocks: None,
    };
    let mut blocks = vec![Block {
        name: "entry".into(),
        instructions: vec![Inst::new(None, Op::Branch("b0".into()))],
    }];
    for (index, &constant) in constants.iter().enumerate() {
        let pair_name = format!("p{index}");
        let value_name = format!("v{index}");
        let flag_name = format!("f{index}");
        let source = if index == 0 {
            "x".into()
        } else {
            format!("v{}", index - 1)
        };
        let next = if index + 1 == constants.len() {
            "exit".into()
        } else {
            format!("b{}", index + 1)
        };
        blocks.push(Block {
            name: format!("b{index}"),
            instructions: vec![
                Inst::new(
                    Some(pair_name.clone()),
                    Op::Call {
                        result: pair.clone(),
                        result_attrs: vec![],
                        callee: Val::Global(intrinsic.clone()),
                        args: vec![
                            Arg::new(ty.clone(), Val::Local(source)),
                            Arg::new(ty.clone(), Val::Int(constant)),
                        ],
                        vararg: None,
                    },
                ),
                Inst::new(
                    Some(value_name),
                    Op::Extract {
                        arg: Arg::new(pair.clone(), Val::Local(pair_name.clone())),
                        index: 0,
                    },
                ),
                Inst::new(
                    Some(flag_name.clone()),
                    Op::Extract {
                        arg: Arg::new(pair.clone(), Val::Local(pair_name)),
                        index: 1,
                    },
                ),
                Inst::new(
                    None,
                    Op::CondBranch {
                        condition: Val::Local(flag_name),
                        yes: "trap".into(),
                        no: next,
                    },
                ),
            ],
        });
    }
    blocks.push(Block {
        name: "exit".into(),
        instructions: vec![Inst::new(
            None,
            Op::Return(Some(Arg::new(
                ty.clone(),
                Val::Local(format!("v{}", constants.len() - 1)),
            ))),
        )],
    });
    blocks.push(Block {
        name: "trap".into(),
        instructions: vec![
            Inst::new(
                None,
                Op::Call {
                    result: Ty::Void,
                    result_attrs: vec![],
                    callee: Val::Global("llvm.trap".into()),
                    args: vec![],
                    vararg: None,
                },
            ),
            Inst::new(None, Op::Unreachable),
        ],
    });
    Module {
        triple: "x86_64-unknown-linux-gnu".into(),
        fingerprint: "diagnostic-model-fixture".into(),
        items: (vec![
            Item::Function(declaration("llvm.trap".into(), Ty::Void, vec![])),
            Item::Function(declaration(
                intrinsic,
                pair,
                vec![Param::unnamed(ty.clone()), Param::unnamed(ty.clone())],
            )),
            Item::Function(Function {
                name: "test".into(),
                linkage: Linkage::External,
                signature: Signature::new(ty.clone(), vec![Param::new(ty, "x")]),
                attrs: vec![],
                blocks: Some((blocks).into()),
            }),
        ])
        .into(),
    }
}
fn function(module: &mut Module) -> &mut Function {
    let Item::Function(function) = module.items.last_mut().unwrap() else {
        unreachable!()
    };
    function
}
fn block<'a>(module: &'a mut Module, name: &str) -> &'a mut Block {
    function(module)
        .blocks
        .as_mut()
        .unwrap()
        .iter_mut()
        .find(|b| b.name == name)
        .unwrap()
}
fn with_phi(mut module: Module, checks: usize) -> Module {
    let ty = function(&mut module).signature.result.clone();
    block(&mut module, "exit").instructions = vec![
        Inst::new(
            Some("join".into()),
            Op::Phi {
                ty: ty.clone(),
                incoming: vec![(
                    Val::Local(format!("v{}", checks - 1)),
                    format!("b{}", checks - 1),
                )],
            },
        ),
        Inst::new(
            None,
            Op::Return(Some(Arg::new(ty, Val::Local("join".into())))),
        ),
    ];
    module
}

fn run(mut module: Module, expected: usize) {
    validate_module(&module).unwrap();
    let original = module.clone();
    assert_eq!(fold_checked_add_chains(&mut module), expected);
    validate_module(&module).unwrap();
    if expected == 0 {
        assert_eq!(module, original);
    }
    assert_eq!(fold_checked_add_chains(&mut module), 0);
}
#[test]
fn signed_unsigned_widths_and_literal_bits() {
    for bits in [8, 16, 32, 64] {
        run(tiny(bits, true, &[1, 2, 0]), 2);
        run(tiny(bits, true, &[-1, -2, 0]), 2);
        run(tiny(bits, true, &[0, 0]), 1);
        run(tiny(bits, false, &[1, 2, 0]), 2);
    }
    run(tiny(8, true, &[255, 255]), 1); // bit-pattern -1, -1
    run(tiny(8, false, &[255, 0]), 1);
    run(tiny(8, true, &[-128, 0]), 1);
}
#[test]
fn mixed_sign_and_unrepresentable_totals_stay_unchanged() {
    run(tiny(8, true, &[1, -1]), 0);
    run(tiny(8, true, &[127, 1]), 0);
    run(tiny(8, true, &[-128, -1]), 0);
    run(tiny(8, false, &[255, 1]), 0);
    run(tiny(64, true, &[i64::MAX as i128, 1]), 0);
    run(tiny(64, false, &[u64::MAX as i128, 1]), 0);
}
#[test]
fn effect_inside_candidate_block_prevents_contraction() {
    let mut module = tiny(8, true, &[1, 2]);
    module.items.insert(
        0,
        Item::Function(Function {
            name: "effect".into(),
            linkage: Linkage::External,
            signature: Signature::new(Ty::Void, vec![]),
            attrs: vec![],
            blocks: None,
        }),
    );
    block(&mut module, "b1").instructions.insert(
        0,
        Inst::new(
            None,
            Op::Call {
                result: Ty::Void,
                result_attrs: vec![],
                callee: Val::Global("effect".into()),
                args: vec![],
                vararg: None,
            },
        ),
    );
    run(module, 0);
}
#[test]
fn intermediate_liveout_prevents_contraction() {
    let mut module = tiny(8, true, &[1, 2]);
    block(&mut module, "exit").instructions = vec![
        Inst::new(
            Some("sum".into()),
            Op::Binary {
                opcode: "add".into(),
                ty: Ty::Int(8),
                left: Val::Local("v0".into()),
                right: Val::Local("v1".into()),
            },
        ),
        Inst::new(
            None,
            Op::Return(Some(Arg::new(Ty::Int(8), Val::Local("sum".into())))),
        ),
    ];
    run(module, 0);
}
#[test]
fn endpoint_phi_and_cycle_are_rejected() {
    let mut phi = tiny(8, true, &[1, 2]);
    block(&mut phi, "exit").instructions = vec![
        Inst::new(
            Some("join".into()),
            Op::Phi {
                ty: Ty::Int(8),
                incoming: vec![(Val::Local("v1".into()), "b1".into())],
            },
        ),
        Inst::new(
            None,
            Op::Return(Some(Arg::new(Ty::Int(8), Val::Local("join".into())))),
        ),
    ];
    run(phi, 0);
    let mut cycle = tiny(8, true, &[1, 2]);
    block(&mut cycle, "exit").instructions = vec![Inst::new(None, Op::Branch("b0".into()))];
    run(cycle, 0);
}
#[test]
fn all_original_validation_precedes_mutation() {
    let mut module = tiny(8, true, &[1, 2]);
    module.items.push(Item::Function(Function {
        name: "malformed_unused".into(),
        linkage: Linkage::External,
        signature: Signature::new(Ty::Int(7), vec![]),
        attrs: vec![],
        blocks: None,
    }));
    assert!(validate_module(&module).is_err());
    let original = module.clone();
    assert_eq!(fold_checked_add_chains(&mut module), 0);
    assert_eq!(module, original);
}
#[test]
fn adjacent_nonoverlapping_plans_keep_values_and_edges() {
    run(tiny(8, true, &[1, 2, -1, -2]), 2);
}
#[test]
fn deep_operand_in_same_function_is_bounded_and_unchanged() {
    let mut module = tiny(8, true, &[1, 2]);
    let mut value = Val::Null;
    for _ in 0..10_000 {
        value = Val::Gep {
            ty: Ty::Int(8),
            ptr: Box::new(value),
            indices: vec![Arg::new(Ty::Int(32), Val::Int(0))],
        };
    }
    function(&mut module).blocks.as_mut().unwrap().push(Block {
        name: "unrelated_deep".into(),
        instructions: vec![
            Inst::new(
                Some("deep_value".into()),
                Op::Freeze(Arg::new(Ty::Ptr, value)),
            ),
            Inst::new(None, Op::Unreachable),
        ],
    });
    let before_error = validate_module(&module).unwrap_err();
    let before_blocks = function(&mut module).blocks.as_ref().unwrap().len();
    reset_work();
    assert_eq!(fold_checked_add_chains(&mut module), 0);
    let (nodes, probes) = work();
    assert!(nodes < 400, "bounded scanner visited {nodes} nodes");
    assert_eq!(probes, 0);
    assert_eq!(
        function(&mut module).blocks.as_ref().unwrap().len(),
        before_blocks
    );
    assert_eq!(validate_module(&module).unwrap_err(), before_error);
    assert!(
        matches!(&block(&mut module,"b0").instructions[0].op,Op::Call { args,.. } if args[1].value==Val::Int(1))
    );
    // Even a populated cache must not compare or clone an unvalidated deep
    // operand while selecting a retained output.
    let mut cache = Cache::default();
    assert_eq!(
        fold_checked_add_chains_cached(&mut tiny(8, true, &[1, 2]), &mut cache),
        1
    );
    let previous = cache.epoch.clone().unwrap();
    reset_work();
    assert_eq!(fold_checked_add_chains_cached(&mut module, &mut cache), 0);
    assert_eq!(work(), (0, 0));
    assert!(Arc::ptr_eq(&previous, cache.epoch.as_ref().unwrap()));
    assert_eq!(validate_module(&module).unwrap_err(), before_error);
    // Test-created 10k-deep value must also be destroyed iteratively.
    let Op::Freeze(argument) = &mut block(&mut module, "unrelated_deep").instructions[0].op else {
        unreachable!()
    };
    let mut value = std::mem::replace(&mut argument.value, Val::Null);
    while let Val::Gep { ptr, .. } = value {
        value = *ptr;
    }
    println!("deep operand: nodes={nodes}, chain_probes={probes}, unchanged_error={before_error}");
}
fn phi_work(n: usize, reverse: bool, leading_sum_boundary: bool) -> (usize, usize) {
    let mut constants = vec![0; n];
    if leading_sum_boundary {
        constants[0] = 127;
        constants[1] = 1;
    }
    let mut module = with_phi(tiny(8, true, &constants), n);
    if reverse {
        function(&mut module).blocks.as_mut().unwrap()[1..n + 1].reverse();
    }
    validate_module(&module).unwrap();
    reset_work();
    let removed = fold_checked_add_chains(&mut module);
    let work = work();
    validate_module(&module).unwrap();
    assert!(removed > 0 && removed <= n - 2);
    if !reverse && !leading_sum_boundary {
        assert_eq!(removed, n - 2);
    }
    assert!(
        work.1 <= 4 * n + 4,
        "nonlinear candidate probes: n={n} probes={}",
        work.1
    );
    let last = format!("b{}", n - 1);
    let endpoint = block(&mut module, "exit");
    assert!(matches!(&endpoint.instructions[0].op,Op::Phi { incoming,.. } if incoming[0].1==last));
    println!(
        "phi run: n={n}, reverse={reverse}, boundary={leading_sum_boundary}, nodes={}, probes={}, removed={removed}",
        work.0, work.1
    );
    work
}
#[test]
fn phi_tail_work_scales_linearly_and_preserves_last_block() {
    for (reverse, boundary) in [(false, false), (true, false), (false, true), (true, true)] {
        let small = phi_work(256, reverse, boundary);
        let large = phi_work(512, reverse, boundary);
        assert!(large.0 <= small.0 * 2 + 16);
        assert!(large.1 <= small.1 * 2 + 16);
    }
}
#[test]
fn phi_tail_new_prefix_coverage_keeps_original_edge_value() {
    run(with_phi(tiny(8, true, &[1, 2]), 2), 0);
    run(with_phi(tiny(8, true, &[1, 2, 3]), 3), 1);
    run(with_phi(tiny(8, true, &[-1, -2, -3]), 3), 1);
}

#[test]
fn single_candidate_and_dynamic_constants_remain_unchanged() {
    run(tiny(8, true, &[1]), 0);
    let mut dynamic = tiny(8, true, &[1, 2]);
    for name in ["b0", "b1"] {
        let Op::Call { args, .. } = &mut block(&mut dynamic, name).instructions[0].op else {
            unreachable!()
        };
        args[1].value = Val::Local("x".into());
    }
    run(dynamic, 0);

    let mut malformed = tiny(8, true, &[1, 2]);
    for name in ["b0", "b1"] {
        let Op::Call { result_attrs, .. } = &mut block(&mut malformed, name).instructions[0].op
        else {
            unreachable!()
        };
        result_attrs.push(Attr::ZeroExt);
    }
    let original = malformed.clone();
    let original_error = validate_module(&malformed).unwrap_err();
    assert_eq!(fold_checked_add_chains(&mut malformed), 0);
    assert_eq!(malformed, original);
    assert_eq!(validate_module(&malformed).unwrap_err(), original_error);
}

fn prefix_fixture(kind: &str) -> Module {
    let mut module = tiny(8, true, &[1, 2]);
    if kind == "call" {
        module.items.insert(
            0,
            Item::Function(Function {
                name: "opaque".into(),
                linkage: Linkage::External,
                signature: Signature::new(Ty::Int(8), vec![]),
                attrs: vec![],
                blocks: None,
            }),
        );
    }
    for (name, index) in [("b0", 0), ("b1", 1)] {
        let instructions = &mut block(&mut module, name).instructions;
        let Op::Call { args, .. } = &mut instructions[0].op else {
            unreachable!()
        };
        args[1].value = Val::Local(format!("constant{index}"));
        let op = if kind == "call" {
            Op::Call {
                result: Ty::Int(8),
                result_attrs: vec![],
                callee: Val::Global("opaque".into()),
                args: vec![],
                vararg: None,
            }
        } else {
            Op::Binary {
                opcode: if kind == "opcode" { "sub" } else { "add" }.into(),
                ty: Ty::Int(if kind == "type" { 16 } else { 8 }),
                left: if kind == "left" {
                    Val::Local("x".into())
                } else {
                    Val::Zero
                },
                right: if kind == "right" {
                    Val::Local("x".into())
                } else {
                    Val::Int(index + 1)
                },
            }
        };
        instructions.insert(
            0,
            Inst::new(
                if kind == "unnamed" {
                    None
                } else {
                    Some(format!("constant{index}"))
                },
                op,
            ),
        );
    }
    module
}

// Independent V4 gate fixture. Uses the existing tiny/function/block helpers.
// Both arms contain a complete Check, but there is no Check-to-Check edge.
fn split_pair_fixture() -> Module {
    let mut module = tiny(8, true, &[1, 2]);
    function(&mut module)
        .signature
        .params
        .push(Param::new(Ty::Int(1), "choose"));
    block(&mut module, "entry").instructions = vec![Inst::new(
        None,
        Op::CondBranch {
            condition: Val::Local("choose".into()),
            yes: "b0".into(),
            no: "b1".into(),
        },
    )];
    for (name, index, endpoint) in [("b0", 0, "exit"), ("b1", 1, "right_exit")] {
        let instructions = &mut block(&mut module, name).instructions;
        let Op::Call { args, .. } = &mut instructions[0].op else {
            unreachable!()
        };
        args[0].value = Val::Local("x".into());
        args[1].value = Val::Local(format!("constant{index}"));
        let Op::CondBranch { no, .. } = &mut instructions[3].op else {
            unreachable!()
        };
        *no = endpoint.into();
        instructions.insert(
            0,
            Inst::new(
                Some(format!("constant{index}")),
                Op::Binary {
                    opcode: "add".into(),
                    ty: Ty::Int(8),
                    left: Val::Int(0),
                    right: Val::Int(index + 1),
                },
            ),
        );
    }
    block(&mut module, "exit").instructions = vec![Inst::new(
        None,
        Op::Return(Some(Arg::new(Ty::Int(8), Val::Local("v0".into())))),
    )];
    function(&mut module).blocks.as_mut().unwrap().push(Block {
        name: "right_exit".into(),
        instructions: vec![Inst::new(
            None,
            Op::Return(Some(Arg::new(Ty::Int(8), Val::Local("v1".into())))),
        )],
    });
    module
}

#[test]
fn shallow_prefix_rejections_preserve_module_and_original_error() {
    run(prefix_fixture("valid"), 1);
    for kind in ["call", "opcode", "type", "left", "right", "unnamed"] {
        let mut module = prefix_fixture(kind);
        let original = module.clone();
        let error = validate_module(&module).err();
        assert_eq!(fold_checked_add_chains(&mut module), 0);
        assert_eq!(module, original);
        assert_eq!(validate_module(&module).err(), error);
    }
}

#[test]
fn separate_branch_checks_do_not_trigger_metadata_scan() {
    let original = split_pair_fixture();
    for reverse in [false, true] {
        let mut module = original.clone();
        if reverse {
            function(&mut module).blocks.as_mut().unwrap()[1..].reverse();
        }
        validate_module(&module).unwrap();
        let before = module.clone();
        reset_work();
        assert_eq!(fold_checked_add_chains(&mut module), 0);
        assert_eq!(module, before);
        assert_eq!(work(), (0, 0));
        validate_module(&module).unwrap();
    }
}

fn two_cached_functions() -> Module {
    let mut module = tiny(8, true, &[1, 2, 3]);
    let mut other = function(&mut module).clone();
    other.name = "other".into();
    module.items.push(Item::Function(other));
    module
}

#[test]
fn cached_plans_reuse_complete_outputs_and_only_replan_changed_functions() {
    let original = two_cached_functions();
    let mut expected = original.clone();
    let removed = fold_checked_add_chains(&mut expected);
    let mut cache = Cache::default();
    let mut cold = original.clone();
    reset_work();
    assert_eq!(
        fold_checked_add_chains_cached(&mut cold, &mut cache),
        removed
    );
    assert_eq!(cold, expected);
    assert!(work().0 > 0 && work().1 > 0);
    assert_eq!((cache.checked_functions, cache.reused_functions), (1, 1));

    let mut warm = original.clone();
    warm.fingerprint = "another-current-diagnostic".into();
    reset_work();
    assert_eq!(
        fold_checked_add_chains_cached(&mut warm, &mut cache),
        removed
    );
    assert_eq!(work(), (0, 0));
    assert_eq!((cache.checked_functions, cache.reused_functions), (0, 2));
    expected.fingerprint = warm.fingerprint.clone();
    assert_eq!(warm, expected);
    validate_module(&warm).unwrap();

    let mut edited = original;
    let Op::Call { args, .. } = &mut block(&mut edited, "b1").instructions[0].op else {
        unreachable!()
    };
    args[1].value = Val::Int(4);
    let mut fresh = edited.clone();
    let fresh_removed = fold_checked_add_chains(&mut fresh);
    reset_work();
    assert_eq!(
        fold_checked_add_chains_cached(&mut edited, &mut cache),
        fresh_removed
    );
    assert_eq!(edited, fresh);
    assert_eq!((cache.checked_functions, cache.reused_functions), (1, 1));
    assert_eq!(cache.retained_functions, 2);
    assert!(work().0 > 0 && work().1 > 0);
}

#[test]
fn cached_empty_plans_and_changed_trap_body_keep_the_original_result() {
    let original = tiny(8, true, &[1, -1]);
    let mut cache = Cache::default();
    let mut cold = original.clone();
    assert_eq!(fold_checked_add_chains_cached(&mut cold, &mut cache), 0);
    assert_eq!(cold, original);
    let mut warm = original.clone();
    reset_work();
    assert_eq!(fold_checked_add_chains_cached(&mut warm, &mut cache), 0);
    assert_eq!(work(), (0, 0));
    assert_eq!((cache.checked_functions, cache.reused_functions), (0, 1));
    assert_eq!(warm, original);

    let mut active = tiny(8, true, &[1, 2]);
    let mut folded = active.clone();
    assert_eq!(fold_checked_add_chains_cached(&mut folded, &mut cache), 1);
    block(&mut active, "trap").instructions = vec![Inst::new(
        None,
        Op::Return(Some(Arg::new(Ty::Int(8), Val::Int(0)))),
    )];
    validate_module(&active).unwrap();
    let before = active.clone();
    assert_eq!(fold_checked_add_chains_cached(&mut active, &mut cache), 0);
    assert_eq!(active, before);
    assert_eq!((cache.checked_functions, cache.reused_functions), (1, 0));
}

#[test]
fn cached_plans_bind_actual_declarations_and_target() {
    let original = tiny(8, true, &[1, 2]);
    let mut cache = Cache::default();
    assert_eq!(
        fold_checked_add_chains_cached(&mut original.clone(), &mut cache),
        1
    );

    let mut changed = original.clone();
    changed.items.insert(
        0,
        Item::Function(Function {
            name: "current_declaration".into(),
            linkage: Linkage::External,
            signature: Signature::new(Ty::Void, vec![]),
            attrs: vec![],
            blocks: None,
        }),
    );
    let mut expected = changed.clone();
    assert_eq!(fold_checked_add_chains(&mut expected), 1);
    assert_eq!(fold_checked_add_chains_cached(&mut changed, &mut cache), 1);
    assert_eq!(changed, expected);
    assert_eq!((cache.checked_functions, cache.reused_functions), (1, 0));

    assert_eq!(
        fold_checked_add_chains_cached(&mut original.clone(), &mut cache),
        1
    );
    let mut changed = original.clone();
    changed.triple = "x86_64-pc-windows-msvc".into();
    let mut expected = changed.clone();
    assert_eq!(fold_checked_add_chains(&mut expected), 1);
    assert_eq!(fold_checked_add_chains_cached(&mut changed, &mut cache), 1);
    assert_eq!(changed, expected);
    assert_eq!((cache.checked_functions, cache.reused_functions), (1, 0));

    assert_eq!(
        fold_checked_add_chains_cached(&mut original.clone(), &mut cache),
        1
    );
    let mut header = original.clone();
    function(&mut header).attrs.push(Attr::NoInline);
    let mut expected = header.clone();
    assert_eq!(fold_checked_add_chains(&mut expected), 1);
    assert_eq!(fold_checked_add_chains_cached(&mut header, &mut cache), 1);
    assert_eq!(header, expected);
    assert_eq!((cache.checked_functions, cache.reused_functions), (1, 0));

    let mut missing_trap = original;
    missing_trap.items.remove(0);
    let before = missing_trap.clone();
    assert_eq!(
        fold_checked_add_chains_cached(&mut missing_trap, &mut cache),
        0
    );
    assert_eq!(missing_trap, before);
    assert_eq!(cache.retained_functions, 0);
}

#[test]
fn cached_outputs_cannot_hide_an_unrelated_invalid_current_item() {
    let original = tiny(8, true, &[1, 2]);
    let mut cache = Cache::default();
    assert_eq!(
        fold_checked_add_chains_cached(&mut original.clone(), &mut cache),
        1
    );
    let previous = cache.epoch.clone().unwrap();
    let mut invalid = original.clone();
    invalid.items.push(Item::Function(Function {
        name: "malformed_unused".into(),
        linkage: Linkage::External,
        signature: Signature::new(Ty::Int(7), vec![]),
        attrs: vec![],
        blocks: None,
    }));
    let before = invalid.clone();
    let error = validate_module(&invalid).unwrap_err();
    reset_work();
    assert_eq!(fold_checked_add_chains_cached(&mut invalid, &mut cache), 0);
    assert_eq!(invalid, before);
    assert_eq!(validate_module(&invalid).unwrap_err(), error);
    assert!(Arc::ptr_eq(&previous, cache.epoch.as_ref().unwrap()));
    assert_eq!((cache.checked_functions, cache.reused_functions), (0, 0));
    assert_eq!(work(), (0, 0));
    let mut recovered = original;
    assert_eq!(
        fold_checked_add_chains_cached(&mut recovered, &mut cache),
        1
    );
    assert_eq!(cache.reused_functions, 1);
}

#[test]
fn cache_reorders_by_full_function_and_retains_no_removed_or_previous_epoch() {
    let original = two_cached_functions();
    let mut cache = Cache::default();
    assert_eq!(
        fold_checked_add_chains_cached(&mut original.clone(), &mut cache),
        4
    );
    let old_epoch = Arc::downgrade(cache.epoch.as_ref().unwrap());
    let mut reordered = original.clone();
    reordered.items.swap(2, 3);
    let mut expected = reordered.clone();
    assert_eq!(fold_checked_add_chains(&mut expected), 4);
    assert_eq!(
        fold_checked_add_chains_cached(&mut reordered, &mut cache),
        4
    );
    assert_eq!(reordered, expected);
    assert_eq!((cache.checked_functions, cache.reused_functions), (0, 2));
    assert!(old_epoch.upgrade().is_none());

    let checkpoint = cache.clone();
    let mut reduced = original.clone();
    reduced.items.pop();
    assert_eq!(fold_checked_add_chains_cached(&mut reduced, &mut cache), 2);
    assert_eq!(cache.retained_functions, 1);
    assert_eq!(checkpoint.retained_functions, 2);
    assert!(
        !cache
            .epoch
            .as_ref()
            .unwrap()
            .functions
            .contains_key("other")
    );
    assert!(
        checkpoint
            .epoch
            .as_ref()
            .unwrap()
            .functions
            .contains_key("other")
    );
    let mut restored = original;
    assert_eq!(fold_checked_add_chains_cached(&mut restored, &mut cache), 4);
    // The removed name was not retained. Its actual blocks are shared with
    // the other current function, whose complete input is admitted now.
    assert_eq!((cache.checked_functions, cache.reused_functions), (0, 2));
}

#[test]
fn retention_bound_declines_cache_without_truncating_the_original_fold() {
    let original = tiny(8, true, &[1, 2]);
    let mut cache = Cache::default();
    assert_eq!(
        fold_checked_add_chains_cached(&mut original.clone(), &mut cache),
        1
    );
    let mut large = original;
    while large.items.len() <= MAX_CACHE_ITEMS {
        large.items.push(Item::Function(Function {
            name: format!("cache_bound_{}", large.items.len()),
            linkage: Linkage::External,
            signature: Signature::new(Ty::Void, vec![]),
            attrs: vec![],
            blocks: None,
        }));
    }
    assert!(!cacheable(&large));
    let mut expected = large.clone();
    assert_eq!(fold_checked_add_chains(&mut expected), 1);
    assert_eq!(fold_checked_add_chains_cached(&mut large, &mut cache), 1);
    assert_eq!(large, expected);
    assert_eq!((cache.checked_functions, cache.reused_functions), (1, 0));
    assert_eq!(cache.retained_functions, 0);
    assert!(cache.epoch.is_none());
}
