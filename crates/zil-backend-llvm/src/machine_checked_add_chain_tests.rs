//! Bounded machine-model regressions; source/native and formal quality remain separate.
use super::*;
use zil_llvm_api::model::{Attr, Linkage, Param, Signature};
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
        items: vec![
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
                blocks: Some(blocks),
            }),
        ],
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
