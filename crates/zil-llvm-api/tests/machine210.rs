use zil_llvm_api::{model::*, validate_module};
fn inst(n: &str, op: Op) -> Inst {
    Inst::new((!n.is_empty()).then(|| n.into()), op)
}
fn ret(v: impl Into<Val>) -> Inst {
    inst("", Op::Return(Some(Arg::new(Ty::Int(32), v))))
}
fn function(blocks: Vec<Block>) -> Function {
    Function {
        name: "answer".into(),
        linkage: Linkage::External,
        signature: Signature::new(Ty::Int(32), vec![Param::new(Ty::Int(1), "c")]),
        attrs: vec![],
        blocks: Some(blocks),
    }
}
fn block(n: &str, instructions: Vec<Inst>) -> Block {
    Block {
        name: n.into(),
        instructions,
    }
}
fn module(f: Function) -> Module {
    Module {
        triple: "x86_64-unknown-linux-gnu".into(),
        fingerprint: "machine-test".into(),
        items: vec![Item::Function(f)],
    }
}
fn diamond() -> Module {
    module(function(vec![
        block(
            "entry",
            vec![inst(
                "",
                Op::CondBranch {
                    condition: Val::Local("c".into()),
                    yes: "left".into(),
                    no: "right".into(),
                },
            )],
        ),
        block(
            "left",
            vec![
                inst(
                    "x",
                    Op::Binary {
                        opcode: "add".into(),
                        ty: Ty::Int(32),
                        left: Val::Int(20),
                        right: Val::Int(22),
                    },
                ),
                inst("", Op::Branch("join".into())),
            ],
        ),
        block("right", vec![inst("", Op::Branch("join".into()))]),
        block(
            "join",
            vec![
                inst(
                    "v",
                    Op::Phi {
                        ty: Ty::Int(32),
                        incoming: vec![
                            (Val::Local("x".into()), "left".into()),
                            (Val::Int(7), "right".into()),
                        ],
                    },
                ),
                ret("%v"),
            ],
        ),
    ]))
}
fn blocks(m: &mut Module) -> &mut Vec<Block> {
    let Item::Function(f) = &mut m.items[0] else {
        panic!()
    };
    f.blocks.as_mut().unwrap()
}
fn reject(mut m: Module, edit: impl FnOnce(&mut Module), message: &str) {
    edit(&mut m);
    let error = validate_module(&m).unwrap_err();
    assert!(error.message.contains(message), "{error}");
}
#[test]
fn valid_phi_and_forward_block_order() {
    let mut m = diamond();
    validate_module(&m).unwrap();
    blocks(&mut m).swap(1, 3);
    validate_module(&m).unwrap();
}
#[test]
fn non_dominating_branch_value_rejected() {
    reject(
        diamond(),
        |m| blocks(m)[3].instructions = vec![ret("%x")],
        "does not dominate",
    );
}
#[test]
fn phi_wrong_predecessor_rejected() {
    reject(
        diamond(),
        |m| {
            if let Op::Phi { incoming, .. } = &mut blocks(m)[3].instructions[0].op {
                incoming[1].1 = "entry".into();
            }
        },
        "predecessor multiset",
    );
}
#[test]
fn phi_type_mismatch_rejected() {
    reject(
        diamond(),
        |m| {
            if let Op::Phi { ty, .. } = &mut blocks(m)[3].instructions[0].op {
                *ty = Ty::Int(64);
            }
        },
        "SSA type mismatch",
    );
}
#[test]
fn missing_definition_and_forward_local_use_rejected() {
    reject(
        diamond(),
        |m| {
            blocks(m)[1].instructions[0].op = Op::Binary {
                opcode: "add".into(),
                ty: Ty::Int(32),
                left: Val::Local("unknown".into()),
                right: Val::Int(1),
            }
        },
        "undefined SSA",
    );
    reject(
        diamond(),
        |m| {
            blocks(m)[1].instructions[0].op = Op::Binary {
                opcode: "add".into(),
                ty: Ty::Int(32),
                left: Val::Local("x".into()),
                right: Val::Int(1),
            }
        },
        "does not dominate",
    );
}
#[test]
fn malformed_cfg_and_duplicate_names_rejected() {
    reject(
        diamond(),
        |m| {
            blocks(m)[0].instructions.clear();
        },
        "unterminated",
    );
    reject(
        diamond(),
        |m| blocks(m)[2].instructions.push(ret(0)),
        "after terminator",
    );
    reject(
        diamond(),
        |m| blocks(m)[2].instructions = vec![inst("", Op::Branch("missing".into()))],
        "unknown block",
    );
    reject(
        diamond(),
        |m| blocks(m)[2].instructions = vec![inst("", Op::Branch("entry".into()))],
        "entry block",
    );
    reject(
        diamond(),
        |m| blocks(m)[2].name = "left".into(),
        "duplicate block",
    );
    reject(
        diamond(),
        |m| blocks(m)[1].instructions[0].result = Some("c".into()),
        "duplicate SSA",
    );
    reject(
        diamond(),
        |m| blocks(m)[1].instructions[0].result = Some("join".into()),
        "block",
    );
}
#[test]
fn void_and_duplicate_global_and_recursive_type_rejected() {
    reject(
        diamond(),
        |m| m.items.push(m.items[0].clone()),
        "duplicate/invalid global",
    );
    reject(
        diamond(),
        |m| {
            m.items.push(Item::NamedType(
                "cycle".into(),
                vec![Ty::Named("cycle".into())],
            ))
        },
        "recursive inline",
    );
    reject(
        diamond(),
        |m| m.items.push(Item::NamedType("bad".into(), vec![Ty::Void])),
        "void is not",
    );
    reject(
        diamond(),
        |m| {
            m.items
                .push(Item::NamedType("bad".into(), vec![Ty::Int(0)]))
        },
        "integer width",
    );
}
#[test]
fn type_indices_alignment_and_constants_checked_before_ffi() {
    for op in [
        Op::Alloca {
            ty: Ty::Int(32),
            align: 3,
        },
        Op::Load {
            ty: Ty::Int(32),
            ptr: Val::Int(1),
            align: 4,
        },
        Op::Gep {
            ty: Ty::Struct(vec![Ty::Int(32)]),
            ptr: Val::Null,
            indices: vec![Arg::new(Ty::Int(32), 0), Arg::new(Ty::Int(32), 1)],
        },
        Op::Extract {
            arg: Arg::new(Ty::Struct(vec![]), Val::Zero),
            index: 0,
        },
        Op::Cast {
            opcode: "zext".into(),
            arg: Arg::new(Ty::Int(32), 1),
            to: Ty::Int(8),
        },
        Op::Binary {
            opcode: "add".into(),
            ty: Ty::Int(8),
            left: Val::Int(256),
            right: Val::Int(0),
        },
        Op::Binary {
            opcode: "invented".into(),
            ty: Ty::Int(32),
            left: Val::Int(0),
            right: Val::Int(0),
        },
    ] {
        let m = module(function(vec![block(
            "entry",
            vec![inst("bad", op), ret(42)],
        )]));
        assert!(validate_module(&m).is_err());
    }
}
#[test]
fn bad_global_initializers_rejected() {
    for (ty, value) in [
        (Ty::array(2, Ty::Int(8)), Val::Bytes(vec![1])),
        (
            Ty::array(2, Ty::Int(8)),
            Val::Array(vec![Arg::new(Ty::Int(8), 1)]),
        ),
        (Ty::Ptr, Val::Global("absent".into())),
        (Ty::Int(32), Val::Local("x".into())),
    ] {
        reject(
            diamond(),
            |m| {
                m.items.push(Item::Global(Global {
                    name: "bad".into(),
                    linkage: Linkage::Private,
                    ty,
                    initializer: Some(value),
                    constant: true,
                    unnamed_addr: false,
                    alignment: 0,
                }))
            },
            "",
        );
    }
}
#[test]
fn direct_call_signature_and_narrow_abi_checked() {
    let mut m = module(function(vec![block(
        "entry",
        vec![
            inst(
                "out",
                Op::Call {
                    result: Ty::Int(32),
                    result_attrs: vec![],
                    callee: Val::Global("external".into()),
                    args: vec![Arg::new(Ty::Int(8), 3).attributed(vec![Attr::ZeroExt])],
                    vararg: None,
                },
            ),
            ret("%out"),
        ],
    )]));
    let mut parameter = Param::unnamed(Ty::Int(8));
    parameter.attrs.push(Attr::ZeroExt);
    m.items.push(Item::Function(Function {
        name: "external".into(),
        linkage: Linkage::External,
        signature: Signature::new(Ty::Int(32), vec![parameter]),
        attrs: vec![Attr::NonLazyBind],
        blocks: None,
    }));
    validate_module(&m).unwrap();
    reject(
        m,
        |m| {
            if let Op::Call { args, .. } = &mut blocks(m)[0].instructions[0].op {
                args[0].attrs.clear();
            }
        },
        "ABI mismatch",
    );
}
#[test]
fn duplicate_predecessor_phi_requires_same_values() {
    let good = module(function(vec![
        block(
            "entry",
            vec![inst(
                "",
                Op::CondBranch {
                    condition: Val::Local("c".into()),
                    yes: "join".into(),
                    no: "join".into(),
                },
            )],
        ),
        block(
            "join",
            vec![
                inst(
                    "v",
                    Op::Phi {
                        ty: Ty::Int(32),
                        incoming: vec![
                            (Val::Int(1), "entry".into()),
                            (Val::Int(1), "entry".into()),
                        ],
                    },
                ),
                ret("%v"),
            ],
        ),
    ]));
    validate_module(&good).unwrap();
    reject(
        good,
        |m| {
            if let Op::Phi { incoming, .. } = &mut blocks(m)[1].instructions[0].op {
                incoming[1].0 = Val::Int(2);
            }
        },
        "different values",
    );
}
#[test]
fn structure_assembly_cannot_silently_drop_events() {
    assert!(Module::from_code("x".into(), "x".into(), Code::from(Event::End)).is_err());
    assert!(
        Module::from_code("x".into(), "x".into(), Code::from(Event::Label("x".into()))).is_err()
    );
    assert!(
        Module::from_code(
            "x".into(),
            "x".into(),
            Code::from(Event::Begin(function(vec![])))
        )
        .is_err()
    );
}
