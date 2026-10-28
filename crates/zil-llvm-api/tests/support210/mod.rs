#![allow(dead_code)]
use zil_llvm_api::model::*;
pub fn i(name: &str, op: Op) -> Inst {
    Inst::new((!name.is_empty()).then(|| name.into()), op)
}
pub fn a(ty: Ty, v: impl Into<Val>) -> Arg {
    Arg::new(ty, v)
}
pub fn b(name: &str, instructions: Vec<Inst>) -> Block {
    Block {
        name: name.into(),
        instructions,
    }
}
pub fn f(name: &str, params: Vec<Param>, result: Ty, blocks: Vec<Block>) -> Function {
    Function {
        name: name.into(),
        linkage: Linkage::External,
        signature: Signature::new(result, params),
        attrs: vec![],
        blocks: Some(blocks),
    }
}
pub fn m(items: Vec<Item>) -> Module {
    Module {
        triple: "x86_64-unknown-linux-gnu".into(),
        fingerprint: "test-210".into(),
        items,
    }
}
pub fn simple() -> Module {
    m(vec![Item::Function(f(
        "main",
        vec![],
        Ty::Int(32),
        vec![b(
            "entry",
            vec![i("", Op::Return(Some(a(Ty::Int(32), 42))))],
        )],
    ))])
}
pub fn loop_module() -> Module {
    m(vec![Item::Function(f(
        "main",
        vec![],
        Ty::Int(32),
        vec![
            b("entry", vec![i("", Op::Branch("loop".into()))]),
            // Forward textual block order and actual loop-carried SSA edge.
            b(
                "exit",
                vec![i("", Op::Return(Some(a(Ty::Int(32), "%next"))))],
            ),
            b(
                "loop",
                vec![
                    i(
                        "n",
                        Op::Phi {
                            ty: Ty::Int(32),
                            incoming: vec![
                                (Val::Int(0), "entry".into()),
                                (Val::Local("next".into()), "loop".into()),
                            ],
                        },
                    ),
                    i(
                        "next",
                        Op::Binary {
                            opcode: "add".into(),
                            ty: Ty::Int(32),
                            left: Val::Local("n".into()),
                            right: Val::Int(1),
                        },
                    ),
                    i(
                        "more",
                        Op::Compare {
                            predicate: "slt".into(),
                            ty: Ty::Int(32),
                            left: Val::Local("next".into()),
                            right: Val::Int(42),
                        },
                    ),
                    i(
                        "",
                        Op::CondBranch {
                            condition: Val::Local("more".into()),
                            yes: "loop".into(),
                            no: "exit".into(),
                        },
                    ),
                ],
            ),
        ],
    ))])
}
pub fn aggregate_module() -> Module {
    let ty = Ty::Named("record".into());
    m(vec![
        Item::NamedType(
            "record".into(),
            vec![Ty::Int(8), Ty::array(3, Ty::Int(16)), Ty::Ptr],
        ),
        Item::Global(Global {
            name: "table".into(),
            linkage: Linkage::Private,
            ty: Ty::array(1, Ty::Ptr),
            initializer: Some(Val::Array(vec![a(Ty::Ptr, Val::Global("answer".into()))])),
            constant: true,
            unnamed_addr: false,
            alignment: 8,
        }),
        Item::Function(f(
            "answer",
            vec![],
            Ty::Int(32),
            vec![b(
                "entry",
                vec![i("", Op::Return(Some(a(Ty::Int(32), 42))))],
            )],
        )),
        Item::Function(f(
            "main",
            vec![],
            Ty::Int(32),
            vec![b(
                "entry",
                vec![
                    i(
                        "r",
                        Op::Alloca {
                            ty: ty.clone(),
                            align: 8,
                        },
                    ),
                    i(
                        "",
                        Op::Store {
                            arg: a(ty.clone(), Val::Zero),
                            ptr: Val::Local("r".into()),
                            align: 8,
                        },
                    ),
                    i(
                        "field",
                        Op::Gep {
                            ty,
                            ptr: Val::Local("r".into()),
                            indices: vec![a(Ty::Int(32), 0), a(Ty::Int(32), 1), a(Ty::Int(64), 2)],
                        },
                    ),
                    i(
                        "",
                        Op::Store {
                            arg: a(Ty::Int(16), 42),
                            ptr: Val::Local("field".into()),
                            align: 2,
                        },
                    ),
                    i(
                        "value",
                        Op::Load {
                            ty: Ty::Int(16),
                            ptr: Val::Local("field".into()),
                            align: 2,
                        },
                    ),
                    i(
                        "extended",
                        Op::Cast {
                            opcode: "zext".into(),
                            arg: a(Ty::Int(16), "%value"),
                            to: Ty::Int(32),
                        },
                    ),
                    i(
                        "fnslot",
                        Op::Gep {
                            ty: Ty::array(1, Ty::Ptr),
                            ptr: Val::Global("table".into()),
                            indices: vec![a(Ty::Int(32), 0), a(Ty::Int(32), 0)],
                        },
                    ),
                    i(
                        "fn",
                        Op::Load {
                            ty: Ty::Ptr,
                            ptr: Val::Local("fnslot".into()),
                            align: 8,
                        },
                    ),
                    i(
                        "called",
                        Op::Call {
                            result: Ty::Int(32),
                            result_attrs: vec![],
                            callee: Val::Local("fn".into()),
                            args: vec![],
                            vararg: None,
                        },
                    ),
                    i(
                        "pair",
                        Op::Insert {
                            arg: a(Ty::Struct(vec![Ty::Int(32), Ty::Int(32)]), Val::Zero),
                            field: a(Ty::Int(32), "%called"),
                            index: 1,
                        },
                    ),
                    i(
                        "result",
                        Op::Extract {
                            arg: a(Ty::Struct(vec![Ty::Int(32), Ty::Int(32)]), "%pair"),
                            index: 1,
                        },
                    ),
                    i("", Op::Return(Some(a(Ty::Int(32), "%result")))),
                ],
            )],
        )),
    ])
}
