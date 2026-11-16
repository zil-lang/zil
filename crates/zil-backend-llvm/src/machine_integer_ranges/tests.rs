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
                        "add",
                        call("llvm.sadd.with.overflow.i32", local("x"), Val::Int(3)),
                    ),
                    inst("add.value", extract("add", 0)),
                    inst("add.overflow", extract("add", 1)),
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
    let mut differences = Vec::new();
    for (a, b) in before.items.iter().zip(m.items.iter()) {
        if let (Item::Function(a), Item::Function(b)) = (a, b) {
            assert_eq!(a.signature, b.signature);
            assert_eq!(a.name, b.name);
            for (x, y) in a.blocks.iter().flatten().zip(b.blocks.iter().flatten()) {
                assert_eq!(x.name, y.name);
                for (i, j) in x.instructions.iter().zip(&y.instructions) {
                    if i != j {
                        differences.push(i.result.clone().unwrap());
                        assert!(
                            matches!(&j.op,Op::Binary {opcode,ty:Ty::Int(1),left:Val::Bool(false),right:Val::Bool(false)}if opcode=="or")
                        );
                    }
                }
            }
        }
    }
    assert_eq!(differences, vec!["flag1", "flag2", "flag3"]);
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
