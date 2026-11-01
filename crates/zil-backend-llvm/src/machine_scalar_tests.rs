//! Machine-model controls; these do not substitute for source/native controls.
use super::*;
use zil_llvm_api::model::{Attr, Block, Inst, Linkage, Param, Signature};

fn local(name: &str) -> Val {
    Val::Local(name.into())
}
fn global(name: &str) -> Val {
    Val::Global(name.into())
}
fn inst(name: Option<&str>, op: Op) -> Inst {
    Inst::new(name.map(str::to_string), op)
}
fn block(name: &str, instructions: Vec<Inst>) -> Block {
    Block {
        name: name.into(),
        instructions,
    }
}
fn function(name: &str, result: Ty, params: Vec<Param>, blocks: Option<Vec<Block>>) -> Item {
    Item::Function(Function {
        name: name.into(),
        linkage: Linkage::External,
        signature: Signature::new(result, params),
        attrs: vec![],
        blocks,
    })
}
fn call(name: &str, result: Ty, args: Vec<Arg>) -> Op {
    Op::Call {
        result,
        result_attrs: vec![],
        callee: global(name),
        args,
        vararg: None,
    }
}
fn ret(ty: Ty, value: Val) -> Inst {
    inst(None, Op::Return(Some(Arg::new(ty, value))))
}
fn sources() -> BTreeSet<String> {
    BTreeSet::from(["leaf".into()])
}
fn model(items: Vec<Item>) -> Module {
    Module {
        triple: "x86_64-unknown-linux-gnu".into(),
        fingerprint: "raw-test-only".into(),
        items,
    }
}
fn machine_function(module: &Module, index: usize) -> &Function {
    let Item::Function(function) = &module.items[index] else {
        panic!("function")
    };
    function
}
fn machine_function_mut(module: &mut Module, index: usize) -> &mut Function {
    let Item::Function(function) = &mut module.items[index] else {
        panic!("function")
    };
    function
}
fn checked_model(bits: u32, operation: &str, left: i128, right: i128) -> Module {
    let ty = Ty::Int(bits);
    let pair = Ty::Struct(vec![ty.clone(), Ty::Int(1)]);
    let intrinsic = format!("llvm.{operation}.with.overflow.i{bits}");
    model(vec![
        function(
            &intrinsic,
            pair.clone(),
            vec![Param::unnamed(ty.clone()), Param::unnamed(ty.clone())],
            None,
        ),
        function("llvm.trap", Ty::Void, vec![], None),
        function(
            "leaf",
            ty.clone(),
            vec![Param::new(ty.clone(), "x"), Param::new(ty.clone(), "y")],
            Some(vec![
                block("entry", vec![inst(None, Op::Branch("body".into()))]),
                block(
                    "body",
                    vec![
                        inst(
                            Some("pair"),
                            call(
                                &intrinsic,
                                pair.clone(),
                                vec![
                                    Arg::new(ty.clone(), local("x")),
                                    Arg::new(ty.clone(), local("y")),
                                ],
                            ),
                        ),
                        inst(
                            Some("r"),
                            Op::Extract {
                                arg: Arg::new(pair.clone(), local("pair")),
                                index: 0,
                            },
                        ),
                        inst(
                            Some("overflow"),
                            Op::Extract {
                                arg: Arg::new(pair, local("pair")),
                                index: 1,
                            },
                        ),
                        inst(
                            None,
                            Op::CondBranch {
                                condition: local("overflow"),
                                yes: "trap".into(),
                                no: "ok".into(),
                            },
                        ),
                    ],
                ),
                block("ok", vec![ret(ty.clone(), local("r"))]),
                block(
                    "trap",
                    vec![
                        inst(None, call("llvm.trap", Ty::Void, vec![])),
                        inst(None, Op::Unreachable),
                    ],
                ),
            ]),
        ),
        function(
            "caller",
            ty.clone(),
            vec![],
            Some(vec![block(
                "entry",
                vec![
                    inst(
                        Some("a"),
                        Op::Binary {
                            opcode: "add".into(),
                            ty: ty.clone(),
                            left: Val::Int(0),
                            right: Val::Int(left),
                        },
                    ),
                    inst(
                        Some("b"),
                        Op::Binary {
                            opcode: "add".into(),
                            ty: ty.clone(),
                            left: Val::Int(0),
                            right: Val::Int(right),
                        },
                    ),
                    inst(
                        Some("answer"),
                        call(
                            "leaf",
                            ty.clone(),
                            vec![
                                Arg::new(ty.clone(), local("a")),
                                Arg::new(ty.clone(), local("b")),
                            ],
                        ),
                    ),
                    ret(ty, local("answer")),
                ],
            )]),
        ),
    ])
}
fn folded_result(module: &Module) -> Option<Scalar> {
    let op = &machine_function(module, 3).blocks.as_ref().unwrap()[0].instructions[2].op;
    let Op::Freeze(arg) = op else { return None };
    scalar(&arg.value, &arg.ty, &HashMap::new())
}

#[test]
fn checked_arithmetic_all_widths_success_and_overflow() {
    for bits in [8, 16, 32, 64] {
        let signed_max = (1i128 << (bits - 1)) - 1;
        let signed_min = -(1i128 << (bits - 1));
        let unsigned_max = (1i128 << bits) - 1;
        for (operation, a, b, expected) in [
            ("sadd", -4, 7, Some(3)),
            ("ssub", -4, 7, Some(-11)),
            ("smul", -4, 7, Some(-28)),
            ("uadd", 4, 7, Some(11)),
            ("usub", 11, 7, Some(4)),
            ("umul", 4, 7, Some(28)),
            ("sadd", signed_max, 1, None),
            ("ssub", signed_min, 1, None),
            ("smul", signed_min, -1, None),
            ("uadd", unsigned_max, 1, None),
            ("usub", 0, 1, None),
            ("umul", unsigned_max, unsigned_max, None),
            // A signless all-one spelling is -1 for signed intrinsic evaluation.
            ("sadd", unsigned_max, 2, Some(1)),
        ] {
            let mut module = checked_model(bits, operation, a, b);
            zil_llvm_api::validate_module(&module).unwrap();
            let original = module.clone();
            let folded = fold_with_sources(&mut module, &sources());
            assert_eq!(
                folded,
                usize::from(expected.is_some()),
                "{operation} i{bits} {a} {b}"
            );
            if let Some(expected) = expected {
                assert_eq!(
                    folded_result(&module),
                    Some(Scalar::new(bits, expected as u64))
                );
                assert_eq!(module.items[..3], original.items[..3]);
                zil_llvm_api::validate_module(&module).unwrap();
            } else {
                assert_eq!(module, original);
            }
        }
    }
}

#[test]
fn bit_patterns_comparisons_casts_and_freeze_are_exact() {
    let empty = HashMap::new();
    let intrinsics = BTreeMap::new();
    for bits in [1, 8, 16, 32, 64] {
        let ty = Ty::Int(bits);
        assert_eq!(
            scalar(&Val::Int(-1), &ty, &empty),
            scalar(&Val::Int(i128::from(mask(bits))), &ty, &empty)
        );
        for value in [Val::Undef, Val::Poison] {
            assert!(
                known_operation(
                    &Op::Freeze(Arg::new(ty.clone(), value)),
                    &empty,
                    &intrinsics
                )
                .is_none()
            );
        }
        let eq = Op::Compare {
            predicate: "eq".into(),
            ty: ty.clone(),
            left: Val::Int(-1),
            right: Val::Int(i128::from(mask(bits))),
        };
        let Some(Known::Scalar(value)) = known_operation(&eq, &empty, &intrinsics) else {
            panic!("known comparison")
        };
        assert_eq!(value, Scalar::new(1, 1));
        let add = Op::Binary {
            opcode: "add".into(),
            ty,
            left: Val::Int(i128::from(mask(bits))),
            right: Val::Int(1),
        };
        let Some(Known::Scalar(value)) = known_operation(&add, &empty, &intrinsics) else {
            panic!("known wrap")
        };
        assert_eq!(value.raw, 0);
    }
    for (opcode, expected) in [("zext", 255), ("sext", u64::MAX)] {
        let op = Op::Cast {
            opcode: opcode.into(),
            arg: Arg::new(Ty::Int(8), Val::Int(-1)),
            to: Ty::Int(64),
        };
        let Some(Known::Scalar(value)) = known_operation(&op, &empty, &intrinsics) else {
            panic!("cast")
        };
        assert_eq!(value.raw, expected);
    }
    for opcode in ["sdiv", "udiv", "srem", "urem", "shl", "lshr", "ashr"] {
        let op = Op::Binary {
            opcode: opcode.into(),
            ty: Ty::Int(32),
            left: Val::Int(1),
            right: Val::Int(1),
        };
        assert!(known_operation(&op, &empty, &intrinsics).is_none());
    }
}

#[test]
fn preflight_rejection_never_hides_original_machine_errors() {
    for corruption in 0..8 {
        let mut module = checked_model(32, "sadd", 3, 4);
        let instructions = &mut machine_function_mut(&mut module, 3)
            .blocks
            .as_mut()
            .unwrap()[0]
            .instructions;
        let Op::Call {
            result,
            result_attrs,
            callee,
            args,
            ..
        } = &mut instructions[2].op
        else {
            unreachable!()
        };
        match corruption {
            0 => {
                args.pop();
            }
            1 => {
                args[0].ty = Ty::Int(64);
            }
            2 => {
                args[0].attrs.push(Attr::ZeroExt);
            }
            3 => {
                *result = Ty::Int(64);
            }
            4 => {
                args[0].value = local("missing");
            }
            5 => {
                *callee = global("missing");
            }
            6 => {
                result_attrs.push(Attr::ZeroExt);
            }
            7 => {
                args[0].value = local("answer");
            }
            _ => unreachable!(),
        }
        let original = module.clone();
        let before = zil_llvm_api::validate_module(&module)
            .unwrap_err()
            .to_string();
        assert_eq!(fold_with_sources(&mut module, &sources()), 0);
        assert_eq!(module, original);
        assert_eq!(
            zil_llvm_api::validate_module(&module)
                .unwrap_err()
                .to_string(),
            before
        );
    }
    let mut module = checked_model(32, "sadd", 3, 4);
    machine_function_mut(&mut module, 2)
        .blocks
        .as_mut()
        .unwrap()
        .push(block(
            "dead",
            vec![
                inst(Some("bad"), call("absent", Ty::Int(32), vec![])),
                ret(Ty::Int(32), Val::Int(0)),
            ],
        ));
    assert!(zil_llvm_api::validate_module(&module).is_err());
    let original = module.clone();
    assert_eq!(fold_with_sources(&mut module, &sources()), 0);
    assert_eq!(module, original);
}

#[test]
fn unknown_memory_ordinary_call_cycle_and_limits_keep_calls() {
    for reason in 0..6 {
        let mut module = checked_model(32, "sadd", 3, 4);
        match reason {
            0 => {
                let caller = machine_function_mut(&mut module, 3);
                caller
                    .signature
                    .params
                    .push(Param::new(Ty::Int(32), "input"));
                if let Op::Binary { right, .. } =
                    &mut caller.blocks.as_mut().unwrap()[0].instructions[0].op
                {
                    *right = local("input");
                }
            }
            1 => {
                let body = &mut machine_function_mut(&mut module, 2)
                    .blocks
                    .as_mut()
                    .unwrap()[1]
                    .instructions;
                body.insert(
                    0,
                    inst(
                        Some("slot"),
                        Op::Alloca {
                            ty: Ty::Int(32),
                            align: 4,
                        },
                    ),
                );
            }
            2 => {
                module
                    .items
                    .push(function("ordinary", Ty::Int(32), vec![], None));
                machine_function_mut(&mut module, 2)
                    .blocks
                    .as_mut()
                    .unwrap()[1]
                    .instructions
                    .insert(
                        0,
                        inst(Some("effect"), call("ordinary", Ty::Int(32), vec![])),
                    );
            }
            3 => {
                let blocks = machine_function_mut(&mut module, 2)
                    .blocks
                    .as_mut()
                    .unwrap();
                blocks[1].instructions.last_mut().unwrap().op = Op::CondBranch {
                    condition: Val::Bool(false),
                    yes: "body".into(),
                    no: "ok".into(),
                };
            }
            4 => {
                let body = &mut machine_function_mut(&mut module, 2)
                    .blocks
                    .as_mut()
                    .unwrap()[1]
                    .instructions;
                for i in 0..MAX_LEAF_INSTRUCTIONS {
                    body.insert(
                        0,
                        Inst::new(
                            Some(format!("pad{i}")),
                            Op::Freeze(Arg::new(Ty::Int(32), Val::Int(0))),
                        ),
                    );
                }
            }
            5 => {
                let caller = machine_function_mut(&mut module, 3);
                if let Op::Binary { right, .. } =
                    &mut caller.blocks.as_mut().unwrap()[0].instructions[0].op
                {
                    *right = Val::Poison;
                }
            }
            _ => unreachable!(),
        }
        zil_llvm_api::validate_module(&module).unwrap();
        let original = module.clone();
        assert_eq!(
            fold_with_sources(&mut module, &sources()),
            0,
            "reason {reason}"
        );
        assert_eq!(module, original);
    }
}

#[test]
fn arguments_effects_traps_addresses_and_indirect_calls_stay_in_place() {
    let mut module = checked_model(32, "sadd", 3, 4);
    module
        .items
        .push(function("side_effect", Ty::Void, vec![], None));
    let caller = machine_function_mut(&mut module, 3);
    let instructions = &mut caller.blocks.as_mut().unwrap()[0].instructions;
    instructions.insert(0, inst(None, call("side_effect", Ty::Void, vec![])));
    instructions.insert(1, inst(None, call("llvm.trap", Ty::Void, vec![])));
    instructions.insert(
        2,
        inst(
            Some("address"),
            Op::Freeze(Arg::new(Ty::Ptr, global("leaf"))),
        ),
    );
    instructions.insert(
        3,
        inst(
            Some("indirect"),
            Op::Call {
                result: Ty::Int(32),
                result_attrs: vec![],
                callee: local("address"),
                args: vec![
                    Arg::new(Ty::Int(32), Val::Int(3)),
                    Arg::new(Ty::Int(32), Val::Int(4)),
                ],
                vararg: None,
            },
        ),
    );
    zil_llvm_api::validate_module(&module).unwrap();
    let original = module.clone();
    assert_eq!(fold_with_sources(&mut module, &sources()), 1);
    let old = &machine_function(&original, 3).blocks.as_ref().unwrap()[0].instructions;
    let new = &machine_function(&module, 3).blocks.as_ref().unwrap()[0].instructions;
    assert_eq!(old.len(), new.len());
    for i in 0..old.len() {
        if i != 6 {
            assert_eq!(old[i], new[i]);
        }
    }
    assert_eq!(old[6].result, new[6].result);
    assert_eq!(module.items[..3], original.items[..3]);
    zil_llvm_api::validate_module(&module).unwrap();
}

#[test]
fn source_identity_and_intrinsic_definition_are_required() {
    let mut module = checked_model(32, "sadd", 3, 4);
    let original = module.clone();
    assert_eq!(fold_with_sources(&mut module, &BTreeSet::new()), 0);
    assert_eq!(module, original);
    // A function merely named like an intrinsic must not be interpreted as one.
    let intrinsic = machine_function_mut(&mut module, 0);
    intrinsic.signature.params[0].name = Some("a".into());
    intrinsic.signature.params[1].name = Some("b".into());
    intrinsic.blocks = Some(vec![block(
        "entry",
        vec![ret(Ty::Struct(vec![Ty::Int(32), Ty::Int(1)]), Val::Zero)],
    )]);
    // This is a machine-model classification control, not valid LLVM intrinsic IR.
    assert!(overflow_declarations(&module).is_empty());
}

#[test]
fn fuel_and_original_body_boundary_do_not_expand_call_chains() {
    let module = checked_model(32, "sadd", 3, 4);
    let intrinsics = overflow_declarations(&module);
    let leaf = leaf(machine_function(&module, 2), &intrinsics).unwrap();
    let mut fuel = 1;
    assert!(
        evaluate(
            &leaf,
            &[Scalar::new(32, 3), Scalar::new(32, 4)],
            &intrinsics,
            &mut fuel
        )
        .is_none()
    );
    assert_eq!(fuel, 0);
    let mut module = module;
    module.items.push(function(
        "outer",
        Ty::Int(32),
        vec![],
        Some(vec![block(
            "entry",
            vec![
                inst(Some("answer"), call("caller", Ty::Int(32), vec![])),
                ret(Ty::Int(32), local("answer")),
            ],
        )]),
    ));
    let original_outer = module.items[4].clone();
    let candidates = BTreeSet::from(["leaf".into(), "caller".into()]);
    assert_eq!(fold_with_sources(&mut module, &candidates), 1);
    assert_eq!(module.items[4], original_outer);
}

#[test]
fn ignored_overflow_and_non_dominating_argument_are_not_hidden() {
    let mut module = checked_model(32, "sadd", i128::from(i32::MAX), 1);
    machine_function_mut(&mut module, 2)
        .blocks
        .as_mut()
        .unwrap()[1]
        .instructions
        .last_mut()
        .unwrap()
        .op = Op::Branch("ok".into());
    zil_llvm_api::validate_module(&module).unwrap();
    let original = module.clone();
    assert_eq!(fold_with_sources(&mut module, &sources()), 0);
    assert_eq!(module, original);

    let mut module = checked_model(32, "sadd", 3, 4);
    machine_function_mut(&mut module, 3).blocks = Some(vec![
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
            vec![
                inst(
                    Some("sibling"),
                    Op::Freeze(Arg::new(Ty::Int(32), Val::Int(3))),
                ),
                ret(Ty::Int(32), Val::Int(0)),
            ],
        ),
        block(
            "right",
            vec![
                inst(
                    Some("answer"),
                    call(
                        "leaf",
                        Ty::Int(32),
                        vec![
                            Arg::new(Ty::Int(32), local("sibling")),
                            Arg::new(Ty::Int(32), Val::Int(4)),
                        ],
                    ),
                ),
                ret(Ty::Int(32), local("answer")),
            ],
        ),
    ]);
    let original = module.clone();
    let error = zil_llvm_api::validate_module(&module)
        .unwrap_err()
        .to_string();
    assert!(error.contains("does not dominate"));
    assert_eq!(fold_with_sources(&mut module, &sources()), 0);
    assert_eq!(module, original);
}

// Frozen v1 algorithm as an independent behavior reference for every model above.
fn fold_with_sources_v1(module: &mut Module, sources: &BTreeSet<String>) -> usize {
    if sources.is_empty() || zil_llvm_api::validate_module(module).is_err() {
        // Preserve the original machine and original diagnostic/backend order.
        return 0;
    }
    let mut replacements = vec![];
    {
        let intrinsics = overflow_declarations(module);
        // All leaf eligibility is computed from original, validated bodies.
        // Replacing one caller never turns a call chain into a new eligible leaf.
        let leaves = module
            .items
            .iter()
            .filter_map(|item| {
                let Item::Function(function) = item else {
                    return None;
                };
                if !sources.contains(&function.name) {
                    return None;
                }
                Some((function.name.as_str(), leaf(function, &intrinsics)?))
            })
            .collect::<BTreeMap<_, _>>();
        if leaves.is_empty() {
            return 0;
        }
        let mut fuel = MAX_EVALUATION_STEPS;
        'callers: for (item_id, item) in module.items.iter().enumerate() {
            let Item::Function(function) = item else {
                continue;
            };
            let mut values = HashMap::new();
            for (block_id, block) in function.blocks.iter().flatten().enumerate() {
                for (instruction_id, instruction) in block.instructions.iter().enumerate() {
                    if fuel == 0 {
                        break 'callers;
                    }
                    let Some(result_name) = instruction.result.as_deref() else {
                        continue;
                    };
                    let replacement = if let Op::Call {
                        result,
                        result_attrs,
                        callee: Val::Global(name),
                        args,
                        vararg: None,
                    } = &instruction.op
                    {
                        leaves
                            .get(name.as_str())
                            .filter(|leaf| {
                                *result == leaf.function.signature.result
                                    && result_attrs.is_empty()
                                    && args.len() == leaf.function.signature.params.len()
                                    && args
                                        .iter()
                                        .zip(&leaf.function.signature.params)
                                        .all(|(a, p)| a.ty == p.ty && a.attrs == p.attrs)
                            })
                            .and_then(|leaf| {
                                let arguments = args
                                    .iter()
                                    .map(|a| scalar(&a.value, &a.ty, &values))
                                    .collect::<Option<Vec<_>>>()?;
                                evaluate(leaf, &arguments, &intrinsics, &mut fuel)
                            })
                    } else {
                        None
                    };
                    let known = if let Some(value) = replacement {
                        replacements.push((item_id, block_id, instruction_id, value));
                        Some(Known::Scalar(value))
                    } else {
                        // No caller CFG rewriting, speculative branches, phi, load or call effects.
                        known_operation(&instruction.op, &values, &intrinsics)
                    };
                    if let Some(known) = known {
                        values.insert(result_name, known);
                    }
                }
            }
        }
    }
    let count = replacements.len();
    for (item, block, instruction, value) in replacements {
        let Item::Function(function) = &mut module.items[item] else {
            unreachable!()
        };
        function.blocks.as_mut().expect("original definition")[block].instructions[instruction]
            .op = Op::Freeze(Arg::new(Ty::Int(value.bits), value.value()));
    }
    count
}

fn fold_with_sources(module: &mut Module, sources: &BTreeSet<String>) -> usize {
    for budget in [0, 1, 2, 3, 8, 16, 127, 128, 129, MAX_EVALUATION_STEPS] {
        let (mut old_fuel, mut new_fuel) = (budget, budget);
        let expected_plan = plan_with_sources_v2(module, sources, &mut old_fuel);
        let actual_plan = plan_with_sources_and_fuel(module, sources, &mut new_fuel);
        assert_eq!(
            actual_plan, expected_plan,
            "v2/v3 exact ordered plan with fuel {budget}"
        );
        assert_eq!(
            new_fuel, old_fuel,
            "v2/v3 remaining fuel with budget {budget}"
        );
    }
    let mut expected = module.clone();
    let expected_count = fold_with_sources_v1(&mut expected, sources);
    let count = super::fold_leaf_only_with_sources(module, sources);
    assert_eq!(count, expected_count, "v1/v2 replacement count");
    assert_eq!(
        *module, expected,
        "v1/v2 resulting original or transformed model"
    );
    count
}

#[test]
fn unvalidated_planner_is_read_only_and_preserves_malformed_first_errors() {
    for corruption in 0..15 {
        let mut module = checked_model(32, "sadd", 3, 4);
        match corruption {
            0 => module.items.push(module.items[2].clone()),
            1 => {
                machine_function_mut(&mut module, 2)
                    .blocks
                    .as_mut()
                    .unwrap()[1]
                    .name = "entry".into()
            }
            2 => {
                machine_function_mut(&mut module, 2)
                    .blocks
                    .as_mut()
                    .unwrap()[0]
                    .instructions[0]
                    .op = Op::Branch("absent".into())
            }
            3 => machine_function_mut(&mut module, 2).blocks = Some(vec![]),
            4 => machine_function_mut(&mut module, 2)
                .blocks
                .as_mut()
                .unwrap()[0]
                .instructions
                .clear(),
            5 => machine_function_mut(&mut module, 2)
                .blocks
                .as_mut()
                .unwrap()[0]
                .instructions
                .push(ret(Ty::Int(32), Val::Int(9))),
            6 => machine_function_mut(&mut module, 2).signature.params[0].name = None,
            7 => machine_function_mut(&mut module, 2).signature.params[1].name = Some("x".into()),
            8 => {
                machine_function_mut(&mut module, 3)
                    .blocks
                    .as_mut()
                    .unwrap()[0]
                    .instructions[1]
                    .result = Some("a".into())
            }
            9 => {
                machine_function_mut(&mut module, 3)
                    .blocks
                    .as_mut()
                    .unwrap()[0]
                    .instructions[0]
                    .op = Op::Freeze(Arg::new(Ty::Int(0), Val::Int(0)))
            }
            10 => {
                machine_function_mut(&mut module, 3)
                    .blocks
                    .as_mut()
                    .unwrap()[0]
                    .instructions[2]
                    .op = call("absent", Ty::Int(32), vec![])
            }
            11 => {
                machine_function_mut(&mut module, 3)
                    .blocks
                    .as_mut()
                    .unwrap()[0]
                    .instructions[2]
                    .op = call("leaf", Ty::Void, vec![])
            }
            12 => {
                machine_function_mut(&mut module, 2)
                    .blocks
                    .as_mut()
                    .unwrap()[2]
                    .instructions[0] = ret(Ty::Int(8), Val::Int(7))
            }
            13 => {
                machine_function_mut(&mut module, 3)
                    .blocks
                    .as_mut()
                    .unwrap()[0]
                    .instructions[0]
                    .op = Op::Freeze(Arg::new(Ty::Int(32), local("a")))
            }
            14 => {
                machine_function_mut(&mut module, 2)
                    .blocks
                    .as_mut()
                    .unwrap()[0]
                    .instructions[0]
                    .op = Op::CondBranch {
                    condition: Val::Bool(true),
                    yes: "body".into(),
                    no: "absent".into(),
                }
            }
            _ => unreachable!(),
        }
        let original = module.clone();
        let before = zil_llvm_api::validate_module(&module)
            .unwrap_err()
            .to_string();
        let _ = plan_with_sources(&module, &sources());
        assert_eq!(module, original, "planner mutated corruption {corruption}");
        assert_eq!(fold_with_sources(&mut module, &sources()), 0);
        assert_eq!(module, original);
        assert_eq!(
            zil_llvm_api::validate_module(&module)
                .unwrap_err()
                .to_string(),
            before
        );
    }
}

#[test]
fn lazy_planner_ignores_deep_invalid_abi_and_obeys_block_parameter_caps() {
    // A recursive Ty must be rejected through shallow width checks before Ty::eq.
    let mut module = checked_model(32, "sadd", 3, 4);
    let mut deep = Ty::Int(32);
    for _ in 0..200 {
        deep = Ty::Struct(vec![deep]);
    }
    machine_function_mut(&mut module, 2).signature.result = deep.clone();
    if let Op::Call { result, .. } = &mut machine_function_mut(&mut module, 3)
        .blocks
        .as_mut()
        .unwrap()[0]
        .instructions[2]
        .op
    {
        *result = deep;
    }
    let original = module.clone();
    assert!(plan_with_sources(&module, &sources()).is_empty());
    assert_eq!(fold_with_sources(&mut module, &sources()), 0);
    assert_eq!(module, original);
    for count in [MAX_LEAF_BLOCKS, MAX_LEAF_BLOCKS + 1] {
        let mut module = checked_model(32, "sadd", 3, 4);
        let blocks = machine_function_mut(&mut module, 2)
            .blocks
            .as_mut()
            .unwrap();
        for i in blocks.len()..count {
            blocks.push(block(
                &format!("dead{i}"),
                vec![ret(Ty::Int(32), Val::Int(0))],
            ));
        }
        zil_llvm_api::validate_module(&module).unwrap();
        assert_eq!(
            fold_with_sources(&mut module, &sources()),
            usize::from(count == MAX_LEAF_BLOCKS)
        );
    }
    for count in [MAX_LEAF_PARAMETERS, MAX_LEAF_PARAMETERS + 1] {
        let mut module = checked_model(32, "sadd", 3, 4);
        let leaf = machine_function_mut(&mut module, 2);
        for i in leaf.signature.params.len()..count {
            leaf.signature
                .params
                .push(Param::new(Ty::Int(32), format!("p{i}")));
        }
        if let Op::Call { args, .. } = &mut machine_function_mut(&mut module, 3)
            .blocks
            .as_mut()
            .unwrap()[0]
            .instructions[2]
            .op
        {
            for _ in args.len()..count {
                args.push(Arg::new(Ty::Int(32), Val::Int(0)));
            }
        }
        zil_llvm_api::validate_module(&module).unwrap();
        assert_eq!(
            fold_with_sources(&mut module, &sources()),
            usize::from(count == MAX_LEAF_PARAMETERS)
        );
    }
}

// Frozen V2 evaluator/planner reference; only fuel is exposed for observation.
fn known_operation_v2(op: &Op, values: &Values<'_>, intrinsics: &Intrinsics<'_>) -> Option<Known> {
    let value = match op {
        Op::Binary {
            opcode,
            ty,
            left,
            right,
        } => {
            let (a, b) = (scalar(left, ty, values)?, scalar(right, ty, values)?);
            let raw = match opcode.as_str() {
                "add" => a.raw.wrapping_add(b.raw),
                "sub" => a.raw.wrapping_sub(b.raw),
                "mul" => a.raw.wrapping_mul(b.raw),
                "and" => a.raw & b.raw,
                "or" => a.raw | b.raw,
                "xor" => a.raw ^ b.raw,
                _ => return None,
            };
            Scalar::new(a.bits, raw)
        }
        Op::Compare {
            predicate,
            ty,
            left,
            right,
        } => {
            let (a, b) = (scalar(left, ty, values)?, scalar(right, ty, values)?);
            let result = match predicate.as_str() {
                "eq" => a.raw == b.raw,
                "ne" => a.raw != b.raw,
                "ult" => a.raw < b.raw,
                "ule" => a.raw <= b.raw,
                "ugt" => a.raw > b.raw,
                "uge" => a.raw >= b.raw,
                "slt" => a.signed() < b.signed(),
                "sle" => a.signed() <= b.signed(),
                "sgt" => a.signed() > b.signed(),
                "sge" => a.signed() >= b.signed(),
                _ => return None,
            };
            Scalar::new(1, u64::from(result))
        }
        Op::Cast { opcode, arg, to } => {
            let input = scalar(&arg.value, &arg.ty, values)?;
            let bits = width(to)?;
            let raw = match opcode.as_str() {
                "trunc" if bits < input.bits => input.raw,
                "zext" if bits > input.bits => input.raw,
                "sext" if bits > input.bits => input.signed() as u64,
                _ => return None,
            };
            Scalar::new(bits, raw)
        }
        Op::Freeze(arg) => scalar(&arg.value, &arg.ty, values)?,
        Op::Extract { arg, index } => {
            let Val::Local(name) = &arg.value else {
                return None;
            };
            let Known::Overflow(value, overflow) = values.get(name.as_str())? else {
                return None;
            };
            match index {
                0 => *value,
                1 => Scalar::new(1, u64::from(*overflow)),
                _ => return None,
            }
        }
        Op::Call {
            result,
            result_attrs,
            callee: Val::Global(name),
            args,
            vararg,
        } => {
            let intrinsic = *intrinsics.get(name.as_str())?;
            if !result_attrs.is_empty()
                || vararg.is_some()
                || args.len() != 2
                || *result != Ty::Struct(vec![Ty::Int(intrinsic.bits), Ty::Int(1)])
                || args
                    .iter()
                    .any(|a| a.ty != Ty::Int(intrinsic.bits) || !a.attrs.is_empty())
            {
                return None;
            }
            return Some(arithmetic(
                intrinsic,
                scalar(&args[0].value, &args[0].ty, values)?,
                scalar(&args[1].value, &args[1].ty, values)?,
            ));
        }
        _ => return None,
    };
    Some(Known::Scalar(value))
}

fn evaluate_v2(
    leaf: &Leaf<'_>,
    arguments: &[Scalar],
    intrinsics: &Intrinsics<'_>,
    fuel: &mut usize,
) -> Option<Scalar> {
    if arguments.len() != leaf.function.signature.params.len() {
        return None;
    }
    let mut values = HashMap::new();
    for (parameter, &argument) in leaf.function.signature.params.iter().zip(arguments) {
        if width(&parameter.ty) != Some(argument.bits) {
            return None;
        }
        values.insert(parameter.name.as_deref()?, Known::Scalar(argument));
    }
    let blocks = leaf.function.blocks.as_ref()?;
    let mut block = 0;
    let mut steps = 0;
    loop {
        let mut next = None;
        for instruction in &blocks[block].instructions {
            if steps >= MAX_LEAF_INSTRUCTIONS || *fuel == 0 {
                return None;
            }
            steps += 1;
            *fuel -= 1;
            match &instruction.op {
                Op::Branch(label) => next = Some(*leaf.labels.get(label.as_str())?),
                Op::CondBranch { condition, yes, no } => {
                    let condition = scalar(condition, &Ty::Int(1), &values)?;
                    let label = if condition.raw != 0 { yes } else { no };
                    next = Some(*leaf.labels.get(label.as_str())?);
                }
                Op::Return(Some(arg)) => return scalar(&arg.value, &arg.ty, &values),
                // A reached trap or other unknown/effectful operation fails evaluation.
                op => {
                    let known = known_operation_v2(op, &values, intrinsics)?;
                    if matches!(known, Known::Overflow(_, true)) {
                        return None;
                    }
                    values.insert(instruction.result.as_deref()?, known);
                }
            }
        }
        block = next?;
    }
}

fn plan_with_sources_v2(
    module: &Module,
    sources: &BTreeSet<String>,
    fuel: &mut usize,
) -> Vec<Replacement> {
    if sources.is_empty() {
        return vec![];
    }
    let intrinsics = overflow_declarations(module);
    let functions = module
        .items
        .iter()
        .filter_map(|item| {
            let Item::Function(function) = item else {
                return None;
            };
            sources
                .contains(&function.name)
                .then_some((function.name.as_str(), function))
        })
        .collect::<BTreeMap<_, _>>();
    if functions.is_empty() {
        return vec![];
    }
    // Cache eligibility, never results at particular arguments. Every function
    // body stays original until the entire plan has passed original preflight.
    let mut leaves = BTreeMap::<&str, Option<Leaf<'_>>>::new();
    let mut replacements = vec![];
    'callers: for (item_id, item) in module.items.iter().enumerate() {
        let Item::Function(function) = item else {
            continue;
        };
        let mut values = HashMap::new();
        for (block_id, block) in function.blocks.iter().flatten().enumerate() {
            for (instruction_id, instruction) in block.instructions.iter().enumerate() {
                if *fuel == 0 {
                    break 'callers;
                }
                let Some(result_name) = instruction.result.as_deref() else {
                    continue;
                };
                let replacement = if let Op::Call {
                    result,
                    result_attrs,
                    callee: Val::Global(name),
                    args,
                    vararg: None,
                } = &instruction.op
                {
                    functions
                        .get(name.as_str())
                        .filter(|callee| {
                            scalar_signature(callee)
                                && width(result).is_some()
                                && *result == callee.signature.result
                                && result_attrs.is_empty()
                                && args.len() <= MAX_LEAF_PARAMETERS
                                && args.len() == callee.signature.params.len()
                                && args.iter().zip(&callee.signature.params).all(|(a, p)| {
                                    width(&a.ty).is_some() && a.ty == p.ty && a.attrs == p.attrs
                                })
                        })
                        .and_then(|callee| {
                            let arguments = args
                                .iter()
                                .map(|a| scalar(&a.value, &a.ty, &values))
                                .collect::<Option<Vec<_>>>()?;
                            let leaf = leaves
                                .entry(name.as_str())
                                .or_insert_with(|| leaf(callee, &intrinsics))
                                .as_ref()?;
                            evaluate_v2(leaf, &arguments, &intrinsics, fuel)
                        })
                } else {
                    None
                };
                let known = if let Some(value) = replacement {
                    replacements.push((item_id, block_id, instruction_id, value));
                    Some(Known::Scalar(value))
                } else {
                    // No caller CFG rewriting, speculative branches, phi, load or call effects.
                    known_operation_v2(&instruction.op, &values, &intrinsics)
                };
                if let Some(known) = known {
                    values.insert(result_name, known);
                }
            }
        }
    }
    replacements
}

#[test]
fn intrinsic_unknown_fastpath_matches_frozen_v2_for_invalid_shapes() {
    for bits in [8, 16, 32, 64] {
        for operation in ["sadd", "ssub", "smul", "uadd", "usub", "umul"] {
            let module = checked_model(bits, operation, 5, 7);
            let intrinsics = overflow_declarations(&module);
            let name = format!("llvm.{operation}.with.overflow.i{bits}");
            let pair = Ty::Struct(vec![Ty::Int(bits), Ty::Int(1)]);
            for corruption in 0..15 {
                let mut op = call(
                    &name,
                    pair.clone(),
                    vec![
                        Arg::new(Ty::Int(bits), Val::Int(5)),
                        Arg::new(Ty::Int(bits), Val::Int(7)),
                    ],
                );
                let Op::Call {
                    result,
                    result_attrs,
                    args,
                    vararg,
                    ..
                } = &mut op
                else {
                    unreachable!()
                };
                match corruption {
                    0 => {}
                    1 => args.clear(),
                    2 => {
                        args.pop();
                    }
                    3 => args.push(Arg::new(Ty::Int(bits), Val::Int(9))),
                    4 => args[0].value = local("unknown"),
                    5 => args[1].value = local("unknown"),
                    6 => args[0].value = Val::Poison,
                    7 => args[1].value = Val::Undef,
                    8 => args[0].ty = Ty::Int(0),
                    9 => args[1].attrs.push(Attr::ZeroExt),
                    10 => result_attrs.push(Attr::ZeroExt),
                    11 => *vararg = Some(vec![]),
                    12 => *result = Ty::Int(bits),
                    13 => {
                        let mut deep = Ty::Int(bits);
                        for _ in 0..200 {
                            deep = Ty::Struct(vec![deep]);
                        }
                        *result = deep;
                    }
                    14 => {
                        args[0].value = Val::Int(i128::MAX);
                        args[1].value = Val::Int(i128::MIN);
                    }
                    _ => unreachable!(),
                }
                let values = HashMap::new();
                assert_eq!(
                    format!("{:?}", known_operation(&op, &values, &intrinsics)),
                    format!("{:?}", known_operation_v2(&op, &values, &intrinsics)),
                    "{bits} {operation} corruption {corruption}"
                );
            }
        }
    }
}

#[test]
fn caller_gate_keeps_wrappers_unreachable_calls_and_fuel_order() {
    let mut module = checked_model(32, "sadd", 3, 4);
    // Caller is not a source leaf; the gate must inspect its direct source calls.
    let first = module.items[3].clone();
    let mut middle = module.items[2].clone();
    if let Item::Function(function) = &mut middle {
        function.name = "intrinsic-only".into();
    }
    let mut last = first.clone();
    if let Item::Function(function) = &mut last {
        function.name = "wrapper-last".into();
    }
    module.items.push(middle);
    module.items.push(last);
    // Original planner scans even unreachable blocks; preserve that exact scope.
    machine_function_mut(&mut module, 3)
        .blocks
        .as_mut()
        .unwrap()
        .push(block(
            "dead-call",
            vec![
                inst(
                    Some("dead-answer"),
                    call(
                        "leaf",
                        Ty::Int(32),
                        vec![
                            Arg::new(Ty::Int(32), Val::Int(9)),
                            Arg::new(Ty::Int(32), Val::Int(2)),
                        ],
                    ),
                ),
                ret(Ty::Int(32), local("dead-answer")),
            ],
        ));
    zil_llvm_api::validate_module(&module).unwrap();
    assert_eq!(fold_with_sources(&mut module, &sources()), 3);
    // Invalid unnamed scalar call must not be hidden even if it has no proposal.
    let mut invalid = checked_model(32, "sadd", 3, 4);
    machine_function_mut(&mut invalid, 3)
        .blocks
        .as_mut()
        .unwrap()[0]
        .instructions[2]
        .result = None;
    let before = invalid.clone();
    let error = zil_llvm_api::validate_module(&invalid)
        .unwrap_err()
        .to_string();
    assert_eq!(fold_with_sources(&mut invalid, &sources()), 0);
    assert_eq!(invalid, before);
    assert_eq!(
        zil_llvm_api::validate_module(&invalid)
            .unwrap_err()
            .to_string(),
        error
    );
}
