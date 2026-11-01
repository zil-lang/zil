use super::*;
use zil_llvm_api::model::{Linkage, Param, Signature};

fn local(n: &str) -> Val {
    Val::Local(n.into())
}
fn global(n: &str) -> Val {
    Val::Global(n.into())
}
fn inst(n: Option<&str>, op: Op) -> Inst {
    Inst::new(n.map(str::to_string), op)
}
fn ret(bits: u32, v: Val) -> Inst {
    inst(None, Op::Return(Some(Arg::new(Ty::Int(bits), v))))
}
fn block(n: &str, v: Vec<Inst>) -> Block {
    Block {
        name: n.into(),
        instructions: v,
    }
}
fn function(name: &str, bits: u32, params: Vec<Param>, blocks: Option<Vec<Block>>) -> Item {
    Item::Function(Function {
        name: name.into(),
        linkage: Linkage::External,
        signature: Signature::new(Ty::Int(bits), params),
        attrs: vec![],
        blocks,
    })
}
fn call(name: &str, bits: u32, args: Vec<Arg>) -> Op {
    Op::Call {
        result: Ty::Int(bits),
        result_attrs: vec![],
        callee: global(name),
        args,
        vararg: None,
    }
}
fn f<'a>(module: &'a Module, name: &str) -> &'a Function {
    module
        .items
        .iter()
        .find_map(|i| match i {
            Item::Function(f) if f.name == name => Some(f),
            _ => None,
        })
        .unwrap()
}
fn fm<'a>(module: &'a mut Module, name: &str) -> &'a mut Function {
    module
        .items
        .iter_mut()
        .find_map(|i| match i {
            Item::Function(f) if f.name == name => Some(f),
            _ => None,
        })
        .unwrap()
}
fn inputs(bits: u32) -> (Module, BTreeSet<String>) {
    let ty = Ty::Int(bits);
    let module = Module {
        triple: "x86_64-unknown-linux-gnu".into(),
        fingerprint: "summary-model".into(),
        items: vec![
            function(
                "entry",
                bits,
                vec![],
                Some(vec![block(
                    "entry",
                    vec![
                        inst(Some("t"), call("top", bits, vec![])),
                        ret(bits, local("t")),
                    ],
                )]),
            ),
            function(
                "top",
                bits,
                vec![],
                Some(vec![block(
                    "entry",
                    vec![
                        inst(Some("a"), call("aggregate", bits, vec![])),
                        inst(
                            Some("r"),
                            Op::Binary {
                                opcode: "add".into(),
                                ty: ty.clone(),
                                left: local("a"),
                                right: Val::Int(2),
                            },
                        ),
                        ret(bits, local("r")),
                    ],
                )]),
            ),
            function(
                "aggregate",
                bits,
                vec![],
                Some(vec![block(
                    "entry",
                    vec![
                        inst(
                            Some("l"),
                            call("leaf", bits, vec![Arg::new(ty.clone(), Val::Int(3))]),
                        ),
                        inst(
                            Some("r"),
                            Op::Binary {
                                opcode: "add".into(),
                                ty: ty.clone(),
                                left: local("l"),
                                right: Val::Int(9),
                            },
                        ),
                        ret(bits, local("r")),
                    ],
                )]),
            ),
            function(
                "leaf",
                bits,
                vec![Param::new(ty.clone(), "x")],
                Some(vec![block(
                    "entry",
                    vec![
                        inst(
                            Some("r"),
                            Op::Binary {
                                opcode: "add".into(),
                                ty,
                                left: local("x"),
                                right: Val::Int(1),
                            },
                        ),
                        ret(bits, local("r")),
                    ],
                )]),
            ),
        ],
    };
    (
        module,
        BTreeSet::from(["leaf".into(), "aggregate".into(), "top".into()]),
    )
}
fn validate(module: &Module) {
    zil_llvm_api::validate_module(module).unwrap();
}
fn old_plan(module: &Module, sources: &BTreeSet<String>) -> Vec<Replacement> {
    super::super::plan_with_sources(module, sources)
}
fn apply(module: &mut Module, sources: &BTreeSet<String>) -> usize {
    super::super::fold_with_sources(module, sources)
}
fn changed_bodies(module: &Module, sources: &BTreeSet<String>) -> Plan {
    let r = old_plan(module, sources);
    assert!(!r.is_empty());
    plan(module, sources, &r)
}

#[test]
fn reversed_three_level_virtual_summary_preserves_canonical_headers() {
    for bits in [8, 16, 32, 64] {
        let (mut module, sources) = inputs(bits);
        validate(&module);
        let before = module.clone();
        let r = old_plan(&module, &sources);
        assert_eq!(r.len(), 1);
        let p = plan(&module, &sources, &r);
        assert_eq!(p.bodies.len(), 2);
        assert_eq!(p.calls.len(), 1);
        assert_eq!(apply(&mut module, &sources), 4);
        validate(&module);
        assert!(already_constant(
            f(&module, "aggregate"),
            Scalar::new(bits, 13)
        ));
        assert!(already_constant(f(&module, "top"), Scalar::new(bits, 15)));
        assert!(matches!(
            f(&module, "entry").blocks.as_ref().unwrap()[0].instructions[0].op,
            Op::Freeze(_)
        ));
        for name in ["entry", "top", "aggregate", "leaf"] {
            let (a, b) = (f(&before, name), f(&module, name));
            assert_eq!(
                (&a.name, a.linkage, &a.signature, &a.attrs),
                (&b.name, b.linkage, &b.signature, &b.attrs)
            );
        }
        assert_eq!(f(&before, "leaf"), f(&module, "leaf"));
    }
}

#[test]
fn empty_original_plan_skips_new_summary_even_for_large_constant_body() {
    let (module, sources) = inputs(32);
    let p = plan(&module, &sources, &[]);
    assert_eq!(p.count(), 0);
    let mut only = Module {
        items: vec![function(
            "standalone",
            32,
            vec![],
            Some(vec![block("entry", vec![ret(32, Val::Int(42))])]),
        )],
        ..module
    };
    let before = only.clone();
    assert_eq!(apply(&mut only, &BTreeSet::from(["standalone".into()])), 0);
    assert_eq!(only, before);
}

#[test]
fn classification_rejects_effects_in_untaken_reachable_branch() {
    let (mut module, sources) = inputs(32);
    module.items.push(function("foreign", 32, vec![], None));
    fm(&mut module, "aggregate").blocks = Some(vec![
        block(
            "entry",
            vec![inst(
                None,
                Op::CondBranch {
                    condition: Val::Bool(true),
                    yes: "yes".into(),
                    no: "no".into(),
                },
            )],
        ),
        block(
            "yes",
            vec![
                inst(
                    Some("v"),
                    call("leaf", 32, vec![Arg::new(Ty::Int(32), Val::Int(3))]),
                ),
                ret(32, local("v")),
            ],
        ),
        block(
            "no",
            vec![
                inst(Some("effect"), call("foreign", 32, vec![])),
                ret(32, Val::Int(13)),
            ],
        ),
    ]);
    validate(&module);
    assert_eq!(changed_bodies(&module, &sources).count(), 0);
    let before = f(&module, "aggregate").blocks.as_ref().unwrap().len();
    assert_eq!(apply(&mut module, &sources), 1);
    assert_eq!(
        f(&module, "aggregate").blocks.as_ref().unwrap().len(),
        before
    );
    validate(&module);
}

#[test]
fn undef_poison_and_unknown_argument_are_never_virtual_constants() {
    for value in [Val::Undef, Val::Poison] {
        let (mut module, sources) = inputs(32);
        fm(&mut module, "aggregate").blocks.as_mut().unwrap()[0]
            .instructions
            .insert(
                0,
                inst(Some("unknown"), Op::Freeze(Arg::new(Ty::Int(32), value))),
            );
        validate(&module);
        assert_eq!(changed_bodies(&module, &sources).count(), 0);
    }
    let (mut module, mut sources) = inputs(32);
    module.items.push(function(
        "dynamic",
        32,
        vec![Param::new(Ty::Int(32), "arg")],
        Some(vec![block(
            "entry",
            vec![
                inst(
                    Some("v"),
                    call("leaf", 32, vec![Arg::new(Ty::Int(32), local("arg"))]),
                ),
                ret(32, local("v")),
            ],
        )]),
    ));
    sources.insert("dynamic".into());
    let before = f(&module, "dynamic").clone();
    apply(&mut module, &sources);
    assert_eq!(f(&module, "dynamic"), &before);
    validate(&module);
}

#[test]
fn reachable_cfg_and_call_cycles_miss_but_unreachable_valid_cycle_is_removable() {
    let (mut module, sources) = inputs(32);
    fm(&mut module, "aggregate").blocks = Some(vec![
        block(
            "entry",
            vec![
                inst(
                    Some("v"),
                    call("leaf", 32, vec![Arg::new(Ty::Int(32), Val::Int(3))]),
                ),
                inst(
                    None,
                    Op::CondBranch {
                        condition: Val::Bool(true),
                        yes: "exit".into(),
                        no: "loop".into(),
                    },
                ),
            ],
        ),
        block("exit", vec![ret(32, local("v"))]),
        block("loop", vec![inst(None, Op::Branch("loop".into()))]),
    ]);
    validate(&module);
    assert_eq!(changed_bodies(&module, &sources).count(), 0);
    fm(&mut module, "aggregate").blocks.as_mut().unwrap()[0].instructions[1].op =
        Op::Branch("exit".into());
    validate(&module);
    assert!(!changed_bodies(&module, &sources).bodies.is_empty());
    apply(&mut module, &sources);
    assert_eq!(f(&module, "aggregate").blocks.as_ref().unwrap().len(), 1);
    validate(&module);
    let (mut module, sources) = inputs(32);
    fm(&mut module, "aggregate").blocks.as_mut().unwrap()[0]
        .instructions
        .insert(1, inst(Some("cycle"), call("top", 32, vec![])));
    validate(&module);
    assert_eq!(changed_bodies(&module, &sources).count(), 0);
}

#[test]
fn invalid_original_and_call_abi_keep_entire_original_first_error() {
    for bad in [0, 1, 2] {
        let (mut module, sources) = inputs(32);
        match bad {
            0 => {
                fm(&mut module, "entry").blocks.as_mut().unwrap()[0].instructions[0].op =
                    call("top", 64, vec![])
            }
            1 => module.items.push(function(
                "unrelated",
                32,
                vec![],
                Some(vec![block("entry", vec![ret(32, local("missing"))])]),
            )),
            _ => module.items.push(module.items[3].clone()),
        }
        let error = zil_llvm_api::validate_module(&module).unwrap_err();
        let before = module.clone();
        assert_eq!(apply(&mut module, &sources), 0);
        assert_eq!(module, before);
        assert_eq!(zil_llvm_api::validate_module(&module).unwrap_err(), error);
    }
}

#[test]
fn exhausted_positive_budget_is_sticky_and_preserves_original_replacements() {
    let mut work = Work(1);
    assert!(work.take(2).is_none());
    assert_eq!(work.0, 0);
    let (module, sources) = inputs(32);
    let before = module.clone();
    let r = old_plan(&module, &sources);
    assert_eq!(r.len(), 1);
    for fuel in [0, 1, 4, 20, 35] {
        let p = bounded_plan(&module, &sources, &r, &mut Work(fuel));
        assert!(p.is_none());
        assert_eq!(old_plan(&module, &sources), r);
        assert_eq!(module, before);
    }
}

#[test]
fn already_constant_spelling_is_not_rewritten_and_function_values_stay_canonical() {
    let (mut module, mut sources) = inputs(32);
    module.items.push(function(
        "zero",
        32,
        vec![],
        Some(vec![block("original", vec![ret(32, Val::Zero)])]),
    ));
    sources.insert("zero".into());
    let before = f(&module, "zero").clone();
    fm(&mut module, "entry").blocks.as_mut().unwrap()[0]
        .instructions
        .insert(
            0,
            inst(Some("ptr"), Op::Freeze(Arg::new(Ty::Ptr, global("top")))),
        );
    validate(&module);
    apply(&mut module, &sources);
    validate(&module);
    assert_eq!(f(&module, "zero"), &before);
    assert!(
        matches!(&f(&module,"entry").blocks.as_ref().unwrap()[0].instructions[0].op,
        Op::Freeze(Arg{value:Val::Global(n),..}) if n=="top")
    );
}

#[test]
fn summary_handles_large_aggregate_without_relaxing_original_leaf_bound() {
    let (mut module, sources) = inputs(32);
    let body = &mut fm(&mut module, "aggregate").blocks.as_mut().unwrap()[0].instructions;
    let ret = body.pop().unwrap();
    for i in 0..200 {
        body.push(inst(
            Some(&format!("extra{i}")),
            Op::Binary {
                opcode: "add".into(),
                ty: Ty::Int(32),
                left: Val::Int(i),
                right: Val::Int(1),
            },
        ));
    }
    body.push(ret);
    validate(&module);
    assert!(leaf(f(&module, "aggregate"), &overflow_declarations(&module)).is_none());
    assert_eq!(old_plan(&module, &sources).len(), 1);
    apply(&mut module, &sources);
    validate(&module);
    assert!(already_constant(
        f(&module, "aggregate"),
        Scalar::new(32, 13)
    ));
    assert_eq!(MAX_LEAF_INSTRUCTIONS, 128);
}

#[test]
fn checked_signed_boundaries_and_reached_trap_are_misses() {
    for (constant, forced_trap, success) in
        [(120, false, true), (124, false, false), (120, true, false)]
    {
        let (mut module, sources) = inputs(8);
        let ty = Ty::Int(8);
        let pair = Ty::Struct(vec![ty.clone(), Ty::Int(1)]);
        module.items.push(Item::Function(Function {
            name: "llvm.sadd.with.overflow.i8".into(),
            linkage: Linkage::External,
            signature: Signature::new(
                pair.clone(),
                vec![Param::unnamed(ty.clone()), Param::unnamed(ty.clone())],
            ),
            attrs: vec![],
            blocks: None,
        }));
        module.items.push(Item::Function(Function {
            name: "llvm.trap".into(),
            linkage: Linkage::External,
            signature: Signature::new(Ty::Void, vec![]),
            attrs: vec![],
            blocks: None,
        }));
        fm(&mut module, "aggregate").blocks = Some(vec![
            block(
                "entry",
                vec![
                    inst(
                        Some("l"),
                        call("leaf", 8, vec![Arg::new(ty.clone(), Val::Int(3))]),
                    ),
                    inst(
                        Some("pair"),
                        Op::Call {
                            result: pair.clone(),
                            result_attrs: vec![],
                            callee: global("llvm.sadd.with.overflow.i8"),
                            args: vec![
                                Arg::new(ty.clone(), local("l")),
                                Arg::new(ty.clone(), Val::Int(constant)),
                            ],
                            vararg: None,
                        },
                    ),
                    inst(
                        Some("r"),
                        Op::Extract {
                            arg: Arg::new(pair.clone(), local("pair")),
                            index: 0,
                        },
                    ),
                    inst(
                        Some("ov"),
                        Op::Extract {
                            arg: Arg::new(pair, local("pair")),
                            index: 1,
                        },
                    ),
                    inst(
                        None,
                        Op::CondBranch {
                            condition: if forced_trap {
                                Val::Bool(true)
                            } else {
                                local("ov")
                            },
                            yes: "trap".into(),
                            no: "ok".into(),
                        },
                    ),
                ],
            ),
            block("ok", vec![ret(8, local("r"))]),
            block(
                "trap",
                vec![
                    inst(
                        None,
                        Op::Call {
                            result: Ty::Void,
                            result_attrs: vec![],
                            callee: global("llvm.trap"),
                            args: vec![],
                            vararg: None,
                        },
                    ),
                    inst(None, Op::Unreachable),
                ],
            ),
        ]);
        validate(&module);
        assert_eq!(
            !changed_bodies(&module, &sources).bodies.is_empty(),
            success
        );
        apply(&mut module, &sources);
        validate(&module);
        assert_eq!(
            f(&module, "aggregate").blocks.as_ref().unwrap().len() == 1,
            success
        );
    }
}

#[test]
fn unadorned_boolean_constants_use_exact_width() {
    let (mut module, sources) = inputs(1);
    fm(&mut module, "leaf").blocks = Some(vec![block("entry", vec![ret(1, local("x"))])]);
    fm(&mut module, "aggregate").blocks = Some(vec![block(
        "entry",
        vec![
            inst(
                Some("v"),
                call("leaf", 1, vec![Arg::new(Ty::Int(1), Val::Bool(true))]),
            ),
            ret(1, local("v")),
        ],
    )]);
    fm(&mut module, "top").blocks = Some(vec![block(
        "entry",
        vec![
            inst(Some("v"), call("aggregate", 1, vec![])),
            ret(1, local("v")),
        ],
    )]);
    validate(&module);
    apply(&mut module, &sources);
    validate(&module);
    assert!(already_constant(f(&module, "top"), Scalar::new(1, 1)));
}
