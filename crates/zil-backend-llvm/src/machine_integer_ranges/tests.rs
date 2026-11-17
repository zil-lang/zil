use super::*;
use zil_llvm_api::model::{Arg, Block, Global, Inst, Param};
fn inst(name: &str, op: Op) -> Inst {
    Inst::new((!name.is_empty()).then(|| name.into()), op)
}
fn arg(v: Val) -> Arg {
    Arg::new(Ty::Int(32), v)
}
fn local(n: &str) -> Val {
    Val::Local(n.into())
}
fn binary(op: &str, a: Val, b: Val) -> Op {
    Op::Binary {
        opcode: op.into(),
        ty: Ty::Int(32),
        left: a,
        right: b,
    }
}
fn ret(v: Val) -> Inst {
    inst("", Op::Return(Some(arg(v))))
}
fn block(name: &str, instructions: Vec<Inst>) -> Block {
    Block {
        name: name.into(),
        instructions,
    }
}
fn function(name: &str, blocks: Vec<Block>) -> Function {
    Function {
        name: name.into(),
        linkage: Linkage::External,
        signature: zil_llvm_api::model::Signature::new(
            Ty::Int(32),
            vec![Param::new(Ty::Int(32), "x")],
        ),
        attrs: vec![],
        blocks: Some(blocks.into()),
    }
}
fn call(name: &str, a: Val, b: Val) -> Op {
    Op::Call {
        result: Ty::Struct(vec![Ty::Int(32), Ty::Int(1)]),
        result_attrs: vec![],
        callee: Val::Global(name.into()),
        args: vec![arg(a), arg(b)],
        vararg: None,
    }
}
fn extract(name: &str, index: u32) -> Op {
    Op::Extract {
        arg: Arg::new(Ty::Struct(vec![Ty::Int(32), Ty::Int(1)]), local(name)),
        index,
    }
}
fn trap() -> Block {
    block(
        "trap",
        vec![
            inst(
                "",
                Op::Call {
                    result: Ty::Void,
                    result_attrs: vec![],
                    callee: Val::Global("llvm.trap".into()),
                    args: vec![],
                    vararg: None,
                },
            ),
            inst("", Op::Unreachable),
        ],
    )
}
fn leaf() -> Function {
    function(
        "leaf",
        vec![
            block(
                "entry",
                vec![
                    inst("r3", binary("srem", local("x"), Val::Int(3))),
                    inst(
                        "condition",
                        Op::Compare {
                            predicate: "eq".into(),
                            ty: Ty::Int(32),
                            left: local("r3"),
                            right: Val::Int(0),
                        },
                    ),
                    inst(
                        "",
                        Op::CondBranch {
                            condition: local("condition"),
                            yes: "multiply".into(),
                            no: "add".into(),
                        },
                    ),
                ],
            ),
            block(
                "multiply",
                vec![
                    inst(
                        "mul",
                        call("llvm.smul.with.overflow.i32", local("x"), Val::Int(2)),
                    ),
                    inst("mul.value", extract("mul", 0)),
                    inst("mul.overflow", extract("mul", 1)),
                    inst(
                        "",
                        Op::CondBranch {
                            condition: local("mul.overflow"),
                            yes: "trap".into(),
                            no: "mul.rest".into(),
                        },
                    ),
                ],
            ),
            block(
                "mul.rest",
                vec![
                    inst("mul.rem", binary("srem", local("mul.value"), Val::Int(251))),
                    inst("", Op::Branch("join".into())),
                ],
            ),
            block(
                "add",
                vec![
                    inst(
                        "add.pair",
                        call("llvm.sadd.with.overflow.i32", local("x"), Val::Int(3)),
                    ),
                    inst("add.value", extract("add.pair", 0)),
                    inst("add.overflow", extract("add.pair", 1)),
                    inst(
                        "",
                        Op::CondBranch {
                            condition: local("add.overflow"),
                            yes: "trap".into(),
                            no: "add.rest".into(),
                        },
                    ),
                ],
            ),
            block(
                "add.rest",
                vec![
                    inst("add.rem", binary("srem", local("add.value"), Val::Int(251))),
                    inst("", Op::Branch("join".into())),
                ],
            ),
            block(
                "join",
                vec![
                    inst(
                        "result",
                        Op::Phi {
                            ty: Ty::Int(32),
                            incoming: vec![
                                (local("mul.rem"), "mul.rest".into()),
                                (local("add.rem"), "add.rest".into()),
                            ],
                        },
                    ),
                    ret(local("result")),
                ],
            ),
            trap(),
        ],
    )
}
fn program() -> Module {
    let mut items = Vec::new();
    for name in ["llvm.sadd.with.overflow.i32", "llvm.smul.with.overflow.i32"] {
        items.push(Item::Function(Function {
            name: name.into(),
            linkage: Linkage::External,
            signature: zil_llvm_api::model::Signature::new(
                Ty::Struct(vec![Ty::Int(32), Ty::Int(1)]),
                vec![Param::new(Ty::Int(32), "a"), Param::new(Ty::Int(32), "b")],
            ),
            attrs: vec![],
            blocks: None,
        }));
    }
    items.push(Item::Function(Function {
        name: "llvm.trap".into(),
        linkage: Linkage::External,
        signature: zil_llvm_api::model::Signature::new(Ty::Void, vec![]),
        attrs: vec![],
        blocks: None,
    }));
    items.push(Item::Function(leaf()));
    let mut blocks = Vec::new();
    let mut previous = local("x");
    for index in 0..4 {
        let name = if index == 0 {
            "entry".into()
        } else {
            format!("step{index}")
        };
        let mut instructions = Vec::new();
        if index > 0 {
            instructions.push(inst(
                &format!("modulus{index}"),
                binary("add", Val::Int(0), Val::Int(65521)),
            ));
            instructions.push(inst(
                &format!("rem{index}"),
                binary("srem", previous, local(&format!("modulus{index}"))),
            ));
            previous = local(&format!("rem{index}"));
        }
        instructions.push(inst(
            &format!("leaf{index}"),
            Op::Call {
                result: Ty::Int(32),
                result_attrs: vec![],
                callee: Val::Global("leaf".into()),
                args: vec![arg(local("x"))],
                vararg: None,
            },
        ));
        instructions.push(inst(
            &format!("sum{index}"),
            call(
                "llvm.sadd.with.overflow.i32",
                previous,
                local(&format!("leaf{index}")),
            ),
        ));
        instructions.push(inst(
            &format!("value{index}"),
            extract(&format!("sum{index}"), 0),
        ));
        instructions.push(inst(
            &format!("flag{index}"),
            extract(&format!("sum{index}"), 1),
        ));
        instructions.push(inst(
            "",
            Op::CondBranch {
                condition: local(&format!("flag{index}")),
                yes: "trap".into(),
                no: if index == 3 {
                    "done".into()
                } else {
                    format!("step{}", index + 1)
                },
            },
        ));
        previous = local(&format!("value{index}"));
        blocks.push(block(&name, instructions));
    }
    blocks.push(block("done", vec![ret(previous)]));
    blocks.push(trap());
    items.push(Item::Function(function("caller", blocks)));
    Module {
        triple: "x86_64-unknown-linux-gnu".into(),
        fingerprint: "ranges".into(),
        items: items.into(),
    }
}
fn apply(module: &mut Module, protected: &BTreeSet<String>) -> usize {
    fold(module, protected, &mut Default::default())
}
fn definition_mut<'a>(module: &'a mut Module, name: &str) -> &'a mut Function {
    module
        .items
        .iter_mut()
        .find_map(|i| match i {
            Item::Function(f) if f.name == name => Some(f),
            _ => None,
        })
        .unwrap()
}
#[test]
fn universal_return_hulls_remove_only_later_safe_flags_and_keep_all_calls_and_traps() {
    let mut m = program();
    zil_llvm_api::validate_module(&m).unwrap();
    let before = m.clone();
    assert_eq!(apply(&mut m, &BTreeSet::new()), 3);
    zil_llvm_api::validate_module(&m).unwrap();
    assert_eq!(before.items.len(), m.items.len());
    for (old, new) in before.items.iter().zip(m.items.iter()) {
        if let (Item::Function(old), Item::Function(new)) = (old, new) {
            assert_eq!(old.name, new.name);
            assert_eq!(old.signature, new.signature);
            assert_eq!(old.attrs, new.attrs);
            assert_eq!(old.linkage, new.linkage);
            let ordinary = |f: &Function| {
                f.blocks
                    .iter()
                    .flatten()
                    .flat_map(|b| &b.instructions)
                    .filter_map(|i| match &i.op {
                        Op::Call {
                            callee: Val::Global(n),
                            ..
                        } if !n.starts_with("llvm.s") => Some(i.op.clone()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
            };
            assert_eq!(ordinary(old), ordinary(new));
        }
    }
    let caller = definition_mut(&mut m, "caller");
    assert!(
        caller.blocks.as_ref().unwrap().len()
            < before
                .items
                .iter()
                .find_map(|i| match i {
                    Item::Function(f) if f.name == "caller" => f.blocks.as_ref().map(|b| b.len()),
                    _ => None,
                })
                .unwrap()
    );
    let remaining: Vec<_> = caller
        .blocks
        .iter()
        .flatten()
        .flat_map(|b| &b.instructions)
        .filter_map(|i| match &i.op {
            Op::Extract { index: 1, .. } => i.result.as_deref(),
            _ => None,
        })
        .collect();
    assert_eq!(remaining, vec!["flag0"]);
}
#[test]
fn changing_a_callee_to_unknown_or_freezing_it_never_lends_a_stale_range() {
    let original = program();
    for freeze in [false, true] {
        let mut m = original.clone();
        let f = definition_mut(&mut m, "leaf");
        f.blocks = Some(
            vec![block(
                "entry",
                if freeze {
                    vec![
                        inst("bounded", binary("srem", local("x"), Val::Int(251))),
                        inst("frozen", Op::Freeze(arg(local("bounded")))),
                        ret(local("frozen")),
                    ]
                } else {
                    vec![ret(local("x"))]
                },
            )]
            .into(),
        );
        zil_llvm_api::validate_module(&m).unwrap();
        let raw = m.clone();
        assert_eq!(apply(&mut m, &BTreeSet::new()), 0);
        assert_eq!(m, raw);
    }
    let mut m = original;
    let f = definition_mut(&mut m, "leaf");
    let blocks = f.blocks.as_mut().unwrap();
    let Op::Phi { incoming, .. } = &mut blocks[5].instructions[0].op else {
        unreachable!()
    };
    incoming[1].0 = local("x");
    zil_llvm_api::validate_module(&m).unwrap();
    let raw = m.clone();
    assert_eq!(apply(&mut m, &BTreeSet::new()), 0);
    assert_eq!(m, raw);
}
#[test]
fn current_protected_address_noinline_and_available_external_roles_are_not_summaries() {
    for mode in ["protected", "address", "noinline", "available"] {
        let mut m = program();
        let mut protected = BTreeSet::new();
        match mode {
            "protected" => {
                protected.insert("leaf".into());
            }
            "address" => m.items.push(Item::Global(Global {
                name: "address".into(),
                linkage: Linkage::Private,
                ty: Ty::Ptr,
                initializer: Some(Val::Global("leaf".into())),
                constant: true,
                unnamed_addr: false,
                alignment: 8,
            })),
            "noinline" => definition_mut(&mut m, "leaf").attrs.push(Attr::NoInline),
            "available" => definition_mut(&mut m, "leaf").linkage = Linkage::AvailableExternally,
            _ => unreachable!(),
        }
        zil_llvm_api::validate_module(&m).unwrap();
        let raw = m.clone();
        assert_eq!(apply(&mut m, &protected), 0, "{mode}");
        assert_eq!(m, raw);
    }
    let mut invalid = program();
    definition_mut(&mut invalid, "caller").signature.result = Ty::Int(64);
    let raw = invalid.clone();
    assert!(zil_llvm_api::validate_module(&invalid).is_err());
    assert_eq!(apply(&mut invalid, &BTreeSet::new()), 0);
    assert_eq!(invalid, raw);
}
#[test]
fn wrapping_and_remainder_boundaries_are_conservative_for_extreme_i32_values() {
    let empty = World {
        intrinsics: BTreeMap::new(),
        returns: BTreeMap::new(),
        arguments: BTreeMap::new(),
    };
    let pairs = HashMap::new();
    let values = Values::new();
    assert_eq!(
        operation(
            &binary("add", Val::Int(i32::MAX as i128), Val::Int(1)),
            &pairs,
            &empty,
            &values
        ),
        constant(i32::MIN as i128)
    );
    for divisor in [1, 3, 251, 65521, i32::MAX as i128] {
        let r = operation(
            &binary("srem", local("unknown"), Val::Int(divisor)),
            &pairs,
            &empty,
            &values,
        );
        for x in [
            i32::MIN as i128,
            -65522,
            -251,
            -1,
            0,
            1,
            251,
            65522,
            i32::MAX as i128,
        ] {
            let y = x % divisor;
            assert!(r.lo <= y && y <= r.hi);
        }
    }
    for divisor in [0, -1, -251] {
        assert_eq!(
            operation(
                &binary("srem", local("unknown"), Val::Int(divisor)),
                &pairs,
                &empty,
                &values
            ),
            Range::TOP
        );
    }
}

fn generated_with_bounded_root(unknown_caller: bool) -> Module {
    let mut module = program();
    for item in module.items.iter_mut() {
        if let Item::Function(f) = item {
            f.name = match f.name.as_str() {
                "leaf" => "zil.fn.1".into(),
                "caller" => "zil.fn.2".into(),
                _ => f.name.clone(),
            };
            for inst in f
                .blocks
                .iter_mut()
                .flatten()
                .flat_map(|b| &mut b.instructions)
            {
                if let Op::Call {
                    callee: Val::Global(name),
                    ..
                } = &mut inst.op
                    && name == "leaf"
                {
                    *name = "zil.fn.1".into();
                }
            }
        }
    }
    module.items.push(Item::Function(Function {
        name: "unknown".into(),
        linkage: Linkage::External,
        signature: zil_llvm_api::model::Signature::new(Ty::Int(32), vec![]),
        attrs: vec![],
        blocks: None,
    }));
    let call_to = |name: &str, v| Op::Call {
        result: Ty::Int(32),
        result_attrs: vec![],
        callee: Val::Global(name.into()),
        args: vec![arg(v)],
        vararg: None,
    };
    let mut root = function(
        "root",
        vec![block(
            "entry",
            vec![
                inst(
                    "input",
                    Op::Call {
                        result: Ty::Int(32),
                        result_attrs: vec![],
                        callee: Val::Global("unknown".into()),
                        args: vec![],
                        vararg: None,
                    },
                ),
                inst("bounded", binary("srem", local("input"), Val::Int(251))),
                inst("result", call_to("zil.fn.2", local("bounded"))),
                ret(local("result")),
            ],
        )],
    );
    root.signature.params.clear();
    module.items.push(Item::Function(root));
    if unknown_caller {
        module.items.push(Item::Function(function(
            "opaque.root",
            vec![block(
                "entry",
                vec![
                    inst("result", call_to("zil.fn.2", local("x"))),
                    ret(local("result")),
                ],
            )],
        )));
    }
    module
}
#[test]
fn closed_argument_ranges_reach_callees_and_all_unknown_callers_still_join() {
    let mut narrow = generated_with_bounded_root(false);
    zil_llvm_api::validate_module(&narrow).unwrap();
    assert_eq!(apply(&mut narrow, &BTreeSet::from(["root".into()])), 6);
    zil_llvm_api::validate_module(&narrow).unwrap();
    for name in ["zil.fn.1", "zil.fn.2"] {
        assert!(
            !definition_mut(&mut narrow, name)
                .blocks
                .iter()
                .flatten()
                .flat_map(|b| &b.instructions)
                .any(|i| matches!(i.op, Op::Extract { index: 1, .. }))
        );
    }
    let mut wide = generated_with_bounded_root(true);
    zil_llvm_api::validate_module(&wide).unwrap();
    assert_eq!(apply(&mut wide, &BTreeSet::from(["root".into()])), 3);
    zil_llvm_api::validate_module(&wide).unwrap();
    assert!(
        definition_mut(&mut wide, "zil.fn.1")
            .blocks
            .iter()
            .flatten()
            .flat_map(|b| &b.instructions)
            .any(|i| matches!(i.op, Op::Extract { index: 1, .. }))
    );
    let mut protected = program();
    assert_eq!(apply(&mut protected, &BTreeSet::from(["caller".into()])), 3);
    zil_llvm_api::validate_module(&protected).unwrap();
}

#[test]
fn range_round_reuse_matches_fresh_rounds_at_exact_work_boundary() {
    let protected = BTreeSet::from(["root".into()]);
    for unknown in [false, true] {
        let raw = generated_with_bounded_root(unknown);
        let run = |limit, reuse| {
            let mut module = raw.clone();
            FACT_EXECUTIONS.with(|n| n.set(0));
            let changed = fold_with_work(
                &mut module,
                &protected,
                &mut Default::default(),
                limit,
                reuse,
            );
            (changed, module, FACT_EXECUTIONS.with(std::cell::Cell::get))
        };
        let (changed, expected, old_walks) = run(MAX_WORK, false);
        assert!(changed > 0);
        let (current, actual, walks) = run(MAX_WORK, true);
        assert_eq!((current, &actual), (changed, &expected));
        assert!(walks < old_walks, "must remove actual fact/CFG walks");
        zil_llvm_api::validate_module(&actual).unwrap();
        // Find the ordinary planner's first complete admissible work budget.
        let (mut low, mut high) = (0, MAX_WORK);
        while low < high {
            let middle = low + (high - low) / 2;
            if run(middle, false).0 == 0 {
                low = middle + 1;
            } else {
                high = middle;
            }
        }
        assert!(low > 0);
        for limit in [0, low - 1, low, low + 1, MAX_WORK] {
            let (a, ma, _) = run(limit, true);
            let (b, mb, _) = run(limit, false);
            assert_eq!((a, ma), (b, mb), "original fee boundary {limit}");
        }
        assert_eq!(run(low - 1, true).0, 0);
        assert_eq!(run(low, true).0, changed);
    }
}

#[test]
fn indexed_facts_match_original_walker_for_each_world_input_and_budget() {
    let module = generated_with_bounded_root(true);
    zil_llvm_api::validate_module(&module).unwrap();
    let mut world = World {
        intrinsics: module
            .items
            .iter()
            .filter_map(|i| match i {
                Item::Function(f) => intrinsic(f).map(|op| (f.name.as_str(), op)),
                _ => None,
            })
            .collect(),
        returns: BTreeMap::new(),
        arguments: BTreeMap::new(),
    };
    plan::BODY_PLANS.with(|n| n.set(0));
    let mut plans = plan::Plans::default();
    let mut owners = BTreeSet::new();
    for round in 0..3 {
        // Reuse the same body index when both universal return facts and the
        // argument domain change. Neither is captured by the static plan.
        if round > 0 {
            world
                .returns
                .insert("zil.fn.1", Range { lo: -250, hi: 250 });
            world.arguments.insert("zil.fn.2", vec![Range::TOP]);
        }
        if round == 2 {
            world.intrinsics.clear();
            world.returns.insert("zil.fn.1", Range::TOP);
            world.arguments.insert("zil.fn.1", vec![Range::TOP]);
        }
        for item in &module.items {
            let Item::Function(f) = item else { continue };
            let Some(blocks) = &f.blocks else { continue };
            owners.insert(std::ptr::from_ref(&**blocks) as usize);
            for inputs in [vec![], vec![Range::TOP], vec![constant(3)]] {
                let mut remaining = MAX_WORK;
                let expected = facts(f, &world, &mut remaining, &inputs);
                assert!(expected.is_some());
                let cost = MAX_WORK - remaining;
                // Every refusal prefix as well as exact N and N+1. On failure
                // no partial facts are published; successful fees match exactly.
                for budget in 0..=cost + 1 {
                    let (mut old_work, mut new_work) = (budget, budget);
                    let old = facts(f, &world, &mut old_work, &inputs);
                    let new = plans.facts(f, &world, &mut new_work, &inputs);
                    assert_eq!(new, old, "{} round {round}, budget {budget}", f.name);
                    if old.is_some() {
                        assert_eq!(new_work, old_work);
                    }
                }
            }
        }
    }
    assert_eq!(plan::BODY_PLANS.with(std::cell::Cell::get), owners.len());
}

#[test]
fn indexed_range_fold_keeps_original_shared_body_budget_and_malformed_preflight() {
    let mut raw = generated_with_bounded_root(false);
    let template = definition_mut(&mut raw, "zil.fn.1").clone();
    for index in 10..18 {
        let mut shared = template.clone();
        shared.name = format!("zil.fn.{index}");
        if index == 17 {
            shared.attrs.push(Attr::NoInline);
        }
        raw.items.push(Item::Function(shared));
    }
    let protected = BTreeSet::from(["root".into(), "zil.fn.16".into()]);
    zil_llvm_api::validate_module(&raw).unwrap();
    let run = |raw: &Module, budget, indexed| {
        let mut actual = raw.clone();
        let mut preflight = crate::machine_preflight::CurrentPreflight::default();
        let changed = fold_with_work(&mut actual, &protected, &mut preflight, budget, indexed);
        (changed, actual, preflight.executions)
    };
    let expected = run(&raw, MAX_WORK, false);
    assert!(expected.0 > 0);
    assert_eq!(run(&raw, MAX_WORK, true), expected);
    let (mut low, mut high) = (0, MAX_WORK);
    while low < high {
        let middle = low + (high - low) / 2;
        if run(&raw, middle, false).0 == 0 {
            low = middle + 1;
        } else {
            high = middle;
        }
    }
    for budget in [0, low - 1, low, low + 1, MAX_WORK] {
        assert_eq!(run(&raw, budget, true), run(&raw, budget, false));
    }
    for malformed in ["header", "intrinsic", "branch", "duplicate"] {
        let mut invalid = raw.clone();
        match malformed {
            "header" => definition_mut(&mut invalid, "zil.fn.10").signature.result = Ty::Int(64),
            "intrinsic" => {
                definition_mut(&mut invalid, "llvm.sadd.with.overflow.i32")
                    .signature
                    .params
                    .pop();
            }
            "branch" => {
                definition_mut(&mut invalid, "zil.fn.10")
                    .blocks
                    .as_mut()
                    .unwrap()[0]
                    .instructions
                    .last_mut()
                    .unwrap()
                    .op = Op::Branch("missing".into());
            }
            "duplicate" => invalid.items.push(Item::Function(template.clone())),
            _ => unreachable!(),
        }
        assert!(
            zil_llvm_api::validate_module(&invalid).is_err(),
            "{malformed}"
        );
        for indexed in [false, true] {
            assert_eq!(run(&invalid, MAX_WORK, indexed), (0, invalid.clone(), 1));
        }
    }
}

#[test]
fn current_class_membership_keeps_all_callers_and_protected_argument_bounds() {
    let rename = |mut module: Module| {
        for item in &mut module.items {
            if let Item::Function(f) = item {
                if f.name == "zil.fn.2" {
                    f.name = "zil.class.current".into();
                }
                for inst in f
                    .blocks
                    .iter_mut()
                    .flatten()
                    .flat_map(|b| &mut b.instructions)
                {
                    if let Op::Call {
                        callee: Val::Global(name),
                        ..
                    } = &mut inst.op
                        && name == "zil.fn.2"
                    {
                        *name = "zil.class.current".into();
                    }
                }
            }
        }
        module
    };
    let protected = BTreeSet::from(["root".into()]);
    let classes = BTreeSet::from(["zil.class.current".into()]);
    let mut without = rename(generated_with_bounded_root(false));
    assert_eq!(fold(&mut without, &protected, &mut Default::default()), 3);
    let mut narrow = rename(generated_with_bounded_root(false));
    assert_eq!(
        fold_classes(&mut narrow, &protected, &classes, &mut Default::default()),
        6
    );
    zil_llvm_api::validate_module(&narrow).unwrap();
    let mut wide = rename(generated_with_bounded_root(true));
    assert_eq!(
        fold_classes(&mut wide, &protected, &classes, &mut Default::default()),
        3
    );
    zil_llvm_api::validate_module(&wide).unwrap();
    let mut protected_class = rename(generated_with_bounded_root(false));
    assert_eq!(
        fold_classes(
            &mut protected_class,
            &BTreeSet::from(["root".into(), "zil.class.current".into()]),
            &classes,
            &mut Default::default()
        ),
        3
    );
}
