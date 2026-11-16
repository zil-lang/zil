use super::*;
use zil_llvm_api::model::{Arg, Inst, Item, Linkage, Module, Param, Signature};

fn inst(name: &str, op: Op) -> Inst {
    Inst::new((!name.is_empty()).then(|| name.into()), op)
}
fn local(name: &str) -> Val {
    Val::Local(name.into())
}
fn arg(value: Val) -> Arg {
    Arg::new(Ty::Int(32), value)
}
fn block(name: &str, instructions: Vec<Inst>) -> Block {
    Block {
        name: name.into(),
        instructions,
    }
}
fn binary(opcode: &str, left: Val, right: Val) -> Op {
    Op::Binary {
        opcode: opcode.into(),
        ty: Ty::Int(32),
        left,
        right,
    }
}
fn ret(value: Val) -> Inst {
    inst("", Op::Return(Some(arg(value))))
}
fn branch(label: &str) -> Inst {
    inst("", Op::Branch(label.into()))
}
fn ordinary(name: &str, input: Val) -> Inst {
    inst(
        name,
        Op::Call {
            result: Ty::Int(32),
            result_attrs: vec![],
            callee: Val::Global("leaf".into()),
            args: vec![arg(input)],
            vararg: None,
        },
    )
}
fn check(pair: &str, left: Val, right: Val, yes: &str, no: &str) -> Vec<Inst> {
    let pair_ty = Ty::Struct(vec![Ty::Int(32), Ty::Int(1)]);
    vec![
        inst(
            pair,
            Op::Call {
                result: pair_ty.clone(),
                result_attrs: vec![],
                callee: Val::Global("llvm.sadd.with.overflow.i32".into()),
                args: vec![arg(left), arg(right)],
                vararg: None,
            },
        ),
        inst(
            &format!("{pair}.value"),
            Op::Extract {
                arg: Arg::new(pair_ty.clone(), local(pair)),
                index: 0,
            },
        ),
        inst(
            &format!("{pair}.flag"),
            Op::Extract {
                arg: Arg::new(pair_ty, local(pair)),
                index: 1,
            },
        ),
        inst(
            "",
            Op::CondBranch {
                condition: local(&format!("{pair}.flag")),
                yes: yes.into(),
                no: no.into(),
            },
        ),
    ]
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
fn function(blocks: Vec<Block>) -> Function {
    Function {
        name: "caller".into(),
        linkage: Linkage::External,
        signature: Signature::new(Ty::Int(32), vec![Param::new(Ty::Int(32), "x")]),
        attrs: vec![],
        blocks: Some(blocks.into()),
    }
}
fn validate(f: &Function) {
    let declaration = |name: &str, result: Ty, params: Vec<Param>| {
        Item::Function(Function {
            name: name.into(),
            linkage: Linkage::External,
            signature: Signature::new(result, params),
            attrs: vec![],
            blocks: None,
        })
    };
    let module = Module {
        triple: "x86_64-unknown-linux-gnu".into(),
        fingerprint: "compact".into(),
        items: vec![
            declaration(
                "llvm.sadd.with.overflow.i32",
                Ty::Struct(vec![Ty::Int(32), Ty::Int(1)]),
                vec![Param::new(Ty::Int(32), "a"), Param::new(Ty::Int(32), "b")],
            ),
            declaration("llvm.trap", Ty::Void, vec![]),
            declaration("leaf", Ty::Int(32), vec![Param::new(Ty::Int(32), "x")]),
            Item::Function(f.clone()),
        ]
        .into(),
    };
    zil_llvm_api::validate_module(&module).unwrap();
}
fn compact(f: &mut Function, flag: &str) {
    validate(f);
    let flags: Vec<_> = f
        .blocks
        .as_ref()
        .unwrap()
        .iter()
        .enumerate()
        .flat_map(|(bi, b)| {
            b.instructions
                .iter()
                .enumerate()
                .filter_map(move |(ii, i)| (i.result.as_deref() == Some(flag)).then_some((bi, ii)))
        })
        .collect();
    assert_eq!(flags.len(), 1);
    let intrinsics = BTreeMap::from([("llvm.sadd.with.overflow.i32".into(), Arithmetic::Add)]);
    assert_eq!(apply(f, &flags, &intrinsics), 1);
    validate(f);
}
fn named<'a>(f: &'a Function, name: &str) -> Option<&'a Inst> {
    f.blocks
        .as_ref()
        .unwrap()
        .iter()
        .flat_map(|b| &b.instructions)
        .find(|i| i.result.as_deref() == Some(name))
}
fn ordinary_calls(f: &Function) -> Vec<Inst> {
    f.blocks
        .as_ref()
        .unwrap()
        .iter()
        .flat_map(|b| &b.instructions)
        .filter(|i| matches!(&i.op, Op::Call { callee: Val::Global(name), .. } if name == "leaf"))
        .cloned()
        .collect()
}

#[test]
fn consumes_safe_pair_and_materializer_preserving_calls_and_live_trap() {
    let mut entry = vec![
        ordinary("before", local("x")),
        inst("bounded", binary("srem", local("before"), Val::Int(251))),
    ];
    entry.extend(check("safe", local("bounded"), Val::Int(1), "trap", "next"));
    let mut next = vec![
        inst("divisor", binary("add", Val::Int(0), Val::Int(65521))),
        inst(
            "remainder",
            binary("srem", local("safe.value"), local("divisor")),
        ),
        ordinary("after", local("remainder")),
    ];
    next.extend(check("unsafe", local("x"), Val::Int(1), "trap", "done"));
    let mut f = function(vec![
        block("entry", entry),
        block("next", next),
        block("done", vec![ret(local("unsafe.value"))]),
        trap(),
    ]);
    let calls = ordinary_calls(&f);
    let header = (
        f.name.clone(),
        f.signature.clone(),
        f.linkage,
        f.attrs.clone(),
    );
    compact(&mut f, "safe.flag");
    assert_eq!(ordinary_calls(&f), calls);
    assert_eq!(
        (
            f.name.clone(),
            f.signature.clone(),
            f.linkage,
            f.attrs.clone()
        ),
        header
    );
    assert!(named(&f, "safe").is_none());
    assert!(named(&f, "safe.flag").is_none());
    assert!(named(&f, "divisor").is_none());
    assert_eq!(
        named(&f, "safe.value").unwrap().op,
        binary("add", local("bounded"), Val::Int(1))
    );
    assert_eq!(
        named(&f, "remainder").unwrap().op,
        binary("srem", local("safe.value"), Val::Int(65521))
    );
    assert!(matches!(named(&f, "unsafe").unwrap().op, Op::Call { .. }));
    assert!(matches!(
        named(&f, "unsafe.flag").unwrap().op,
        Op::Extract { index: 1, .. }
    ));
    let blocks = f.blocks.as_ref().unwrap();
    assert_eq!(blocks.len(), 3);
    assert!(!blocks.iter().any(|b| b.name == "next"));
    assert_eq!(blocks.iter().find(|b| b.name == "trap").unwrap(), &trap());
}

#[test]
fn phi_predecessor_labels_survive_and_phi_edges_are_not_silently_deleted() {
    let original = function(vec![
        block(
            "entry",
            check("safe", Val::Int(2), Val::Int(3), "trap", "next"),
        ),
        block("next", vec![branch("join")]),
        block(
            "join",
            vec![
                inst(
                    "joined",
                    Op::Phi {
                        ty: Ty::Int(32),
                        incoming: vec![(local("safe.value"), "next".into())],
                    },
                ),
                ret(local("joined")),
            ],
        ),
        trap(),
    ]);
    let mut f = original.clone();
    compact(&mut f, "safe.flag");
    let blocks = f.blocks.as_ref().unwrap();
    assert_eq!(blocks.len(), 3);
    assert_eq!(
        blocks[0].instructions.last().unwrap().op,
        Op::Branch("next".into())
    );
    assert!(blocks.iter().any(|b| b.name == "next"));
    assert!(
        matches!(&named(&f, "joined").unwrap().op, Op::Phi { incoming, .. }
        if incoming == &vec![(local("safe.value"), "next".into())])
    );

    let mut f = original;
    f.blocks.as_mut().unwrap()[3] = block(
        "trap",
        vec![
            inst(
                "failure",
                Op::Phi {
                    ty: Ty::Int(32),
                    incoming: vec![(Val::Int(9), "entry".into())],
                },
            ),
            ret(local("failure")),
        ],
    );
    compact(&mut f, "safe.flag");
    assert!(named(&f, "safe.flag").is_some());
    assert!(
        matches!(&f.blocks.as_ref().unwrap()[0].instructions.last().unwrap().op,
        Op::CondBranch { yes, .. } if yes == "trap")
    );
    assert!(
        matches!(&named(&f, "failure").unwrap().op, Op::Phi { incoming, .. }
        if incoming == &vec![(Val::Int(9), "entry".into())])
    );
}

#[test]
fn alternate_predecessor_prevents_successor_merge() {
    let mut f = function(vec![
        block(
            "entry",
            vec![
                inst(
                    "condition",
                    Op::Compare {
                        predicate: "eq".into(),
                        ty: Ty::Int(32),
                        left: local("x"),
                        right: Val::Int(0),
                    },
                ),
                inst(
                    "",
                    Op::CondBranch {
                        condition: local("condition"),
                        yes: "left".into(),
                        no: "right".into(),
                    },
                ),
            ],
        ),
        block(
            "left",
            check("safe", Val::Int(2), Val::Int(3), "trap", "join"),
        ),
        block(
            "right",
            vec![ordinary("ordinary", local("x")), branch("join")],
        ),
        block("join", vec![ret(local("x"))]),
        trap(),
    ]);
    let calls = ordinary_calls(&f);
    compact(&mut f, "safe.flag");
    assert_eq!(ordinary_calls(&f), calls);
    let blocks = f.blocks.as_ref().unwrap();
    assert_eq!(blocks.len(), 4);
    for name in ["left", "right"] {
        assert_eq!(
            blocks
                .iter()
                .find(|b| b.name == name)
                .unwrap()
                .instructions
                .last()
                .unwrap()
                .op,
            Op::Branch("join".into())
        );
    }
    assert!(blocks.iter().any(|b| b.name == "join"));
    assert!(!blocks.iter().any(|b| b.name == "trap"));
}

#[test]
fn duplicate_edges_keep_both_phi_predecessor_entries() {
    let mut f = function(vec![
        block(
            "entry",
            check("safe", Val::Int(2), Val::Int(3), "join", "join"),
        ),
        block(
            "join",
            vec![
                inst(
                    "joined",
                    Op::Phi {
                        ty: Ty::Int(32),
                        incoming: vec![
                            (local("safe.value"), "entry".into()),
                            (local("safe.value"), "entry".into()),
                        ],
                    },
                ),
                ret(local("joined")),
            ],
        ),
    ]);
    compact(&mut f, "safe.flag");
    assert!(
        matches!(&f.blocks.as_ref().unwrap()[0].instructions.last().unwrap().op,Op::CondBranch {yes,no,..}if yes=="join"&&no=="join")
    );
    assert!(
        matches!(&named(&f,"joined").unwrap().op,Op::Phi {incoming,..}if incoming.len()==2&&incoming[0]==incoming[1])
    );
}
