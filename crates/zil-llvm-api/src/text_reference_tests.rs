//! Byte oracle copied from the Stage14 baseline formatter.
use super::*;

fn original(module: &Module) -> String {
    let mut s = format!(
        "; ZIL verified LLVM module\n; verified-fingerprint: {}\ntarget triple = \"{}\"\n\n",
        module.fingerprint, module.triple
    );
    for item in &module.items {
        match item {
            Item::NamedType(n, fields) => {
                if fields.is_empty() {
                    writeln!(s, "%{n} = type {{  }}").unwrap();
                } else {
                    writeln!(s, "%{n} = type {}", Ty::Struct(fields.clone())).unwrap();
                }
            }
            Item::Global(g) => {
                write!(s, "@{} = {}", g.name, g.linkage.prefix()).unwrap();
                if g.initializer.is_none() {
                    s.push_str("external ");
                }
                if g.unnamed_addr {
                    s.push_str("unnamed_addr ");
                }
                write!(
                    s,
                    "{} {}",
                    if g.constant { "constant" } else { "global" },
                    g.ty
                )
                .unwrap();
                if let Some(v) = &g.initializer {
                    write!(s, " {v}").unwrap();
                }
                if g.alignment != 0 {
                    write!(s, ", align {}", g.alignment).unwrap();
                }
                s.push('\n');
            }
            Item::Function(f) => {
                write!(
                    s,
                    "{} {}",
                    if f.blocks.is_some() {
                        "define"
                    } else {
                        "declare"
                    },
                    f.linkage.prefix()
                )
                .unwrap();
                for a in &f.signature.result_attrs {
                    write!(s, "{} ", a.name()).unwrap();
                }
                write!(s, "{} @{}(", f.signature.result, f.name).unwrap();
                for (i, p) in f.signature.params.iter().enumerate() {
                    if i != 0 {
                        s.push_str(", ");
                    }
                    write!(s, "{}", p.ty).unwrap();
                    for a in &p.attrs {
                        write!(s, " {}", a.name()).unwrap();
                    }
                    if let Some(n) = &p.name {
                        write!(s, " %{n}").unwrap();
                    }
                }
                if f.signature.variadic {
                    if !f.signature.params.is_empty() {
                        s.push_str(", ");
                    }
                    s.push_str("...");
                }
                s.push(')');
                for a in &f.attrs {
                    write!(s, " {}", a.name()).unwrap();
                }
                if let Some(blocks) = &f.blocks {
                    s.push_str(" {\n");
                    for b in blocks {
                        writeln!(s, "{}:", b.name).unwrap();
                        for i in &b.instructions {
                            write_inst(&mut s, i);
                        }
                    }
                    s.push_str("}\n");
                } else {
                    s.push('\n');
                }
            }
        }
    }
    s
}
fn write_inst(s: &mut String, i: &Inst) {
    s.push_str("  ");
    if let Some(r) = &i.result {
        write!(s, "%{r} = ").unwrap();
    }
    match &i.op {
        Op::Alloca { ty, align } => {
            write!(s, "alloca {ty}").unwrap();
            if *align != 0 {
                write!(s, ", align {align}").unwrap();
            }
            Ok(())
        }
        Op::Load { ty, ptr, align } => {
            write!(s, "load {ty}, ptr {ptr}").unwrap();
            if *align != 0 {
                write!(s, ", align {align}").unwrap();
            }
            Ok(())
        }
        Op::Store { arg, ptr, align } => {
            write!(s, "store {arg}, ptr {ptr}").unwrap();
            if *align != 0 {
                write!(s, ", align {align}").unwrap();
            }
            Ok(())
        }
        Op::Binary {
            opcode,
            ty,
            left,
            right,
        } => write!(s, "{opcode} {ty} {left}, {right}"),
        Op::Compare {
            predicate,
            ty,
            left,
            right,
        } => write!(s, "icmp {predicate} {ty} {left}, {right}"),
        Op::Cast { opcode, arg, to } => write!(s, "{opcode} {arg} to {to}"),
        Op::Gep { ty, ptr, indices } => {
            write!(s, "getelementptr {ty}, ptr {ptr}").unwrap();
            for a in indices {
                write!(s, ", {a}").unwrap();
            }
            Ok(())
        }
        Op::Extract { arg, index } => write!(s, "extractvalue {arg}, {index}"),
        Op::Insert { arg, field, index } => write!(s, "insertvalue {arg}, {field}, {index}"),
        Op::Call {
            result,
            result_attrs,
            callee,
            args,
            vararg,
        } => {
            s.push_str("call ");
            for a in result_attrs {
                write!(s, "{} ", a.name()).unwrap();
            }
            write!(s, "{result} ").unwrap();
            if let Some(types) = vararg {
                s.push('(');
                for (i, t) in types.iter().enumerate() {
                    if i != 0 {
                        s.push_str(", ");
                    }
                    write!(s, "{t}").unwrap();
                }
                if !types.is_empty() {
                    s.push_str(", ");
                }
                s.push_str("...) ");
            }
            write!(s, "{callee}(").unwrap();
            for (i, a) in args.iter().enumerate() {
                if i != 0 {
                    s.push_str(", ");
                }
                write!(s, "{a}").unwrap();
            }
            s.push(')');
            Ok(())
        }
        Op::Freeze(arg) => write!(s, "freeze {arg}"),
        Op::Phi { ty, incoming } => {
            write!(s, "phi {ty} ").unwrap();
            for (i, (v, b)) in incoming.iter().enumerate() {
                if i != 0 {
                    s.push_str(", ");
                }
                write!(s, "[ {v}, %{b} ]").unwrap();
            }
            Ok(())
        }
        Op::Branch(b) => write!(s, "br label %{b}"),
        Op::CondBranch { condition, yes, no } => {
            write!(s, "br i1 {condition}, label %{yes}, label %{no}")
        }
        Op::Return(Some(a)) => write!(s, "ret {a}"),
        Op::Return(None) => write!(s, "ret void"),
        Op::Unreachable => write!(s, "unreachable"),
    }
    .unwrap();
    s.push('\n');
}

fn type_cases() -> Vec<Ty> {
    vec![
        Ty::Void,
        Ty::Int(0),
        Ty::Int(1),
        Ty::Int(8),
        Ty::Int(16),
        Ty::Int(32),
        Ty::Int(64),
        Ty::Int(127),
        Ty::Int(u32::MAX),
        Ty::Ptr,
        Ty::Array(0, Box::new(Ty::Int(8))),
        Ty::Array(
            u64::MAX,
            Box::new(Ty::Struct(vec![Ty::Ptr, Ty::Named("奇怪.\n%".into())])),
        ),
        Ty::Struct(vec![]),
        Ty::Struct(vec![Ty::Int(32), Ty::Int(1)]),
        Ty::Named("".into()),
        Ty::Named("type.unquoted\"".into()),
    ]
}
fn value_cases() -> Vec<Val> {
    vec![
        Val::Local("".into()),
        Val::Local("v.ñ\n%".into()),
        Val::Global("global\"\\".into()),
        Val::Int(i128::MIN),
        Val::Int(i128::MAX),
        Val::Int(-1),
        Val::Int(0),
        Val::Int(10),
        Val::Bool(false),
        Val::Bool(true),
        Val::Null,
        Val::Zero,
        Val::Undef,
        Val::Poison,
        Val::Array(vec![]),
        Val::Array(vec![
            Arg::new(Ty::Struct(vec![]), Val::Zero).attributed(vec![
                Attr::ZeroExt,
                Attr::NonLazyBind,
                Attr::ZeroExt,
            ]),
        ]),
        Val::Bytes(vec![]),
        Val::Bytes((0..=255).collect()),
        Val::Gep {
            ty: Ty::Int(8),
            ptr: Box::new(Val::Global("table".into())),
            indices: vec![],
        },
        Val::Gep {
            ty: Ty::Array(u64::MAX, Box::new(Ty::Int(8))),
            ptr: Box::new(Val::Gep {
                ty: Ty::Ptr,
                ptr: Box::new(Val::Null),
                indices: vec![Arg::new(Ty::Int(32), Val::Int(-1))],
            }),
            indices: vec![Arg::new(Ty::Int(64), Val::Int(i128::MIN))],
        },
        Val::Invalid("\n<raw:\"\\>".into()),
    ]
}

#[test]
fn direct_atoms_match_unchanged_display_including_integer_edges_and_all_bytes() {
    for ty in type_cases() {
        let mut text = String::new();
        write_type(&mut text, &ty);
        assert_eq!(text, ty.to_string());
    }
    for value in value_cases() {
        let mut text = String::new();
        write_value(&mut text, &value);
        assert_eq!(text, value.to_string());
        let arg = Arg::new(Ty::Struct(vec![Ty::Int(32), Ty::Int(1)]), value).attributed(vec![
            Attr::ZeroExt,
            Attr::NonLazyBind,
            Attr::ZeroExt,
        ]);
        text.clear();
        write_argument(&mut text, &arg);
        assert_eq!(text, arg.to_string());
    }
}

#[test]
fn every_instruction_and_module_form_matches_original_bytes() {
    let arg = Arg::new(Ty::Int(32), Val::Local("v".into())).attributed(vec![
        Attr::ZeroExt,
        Attr::NonLazyBind,
        Attr::ZeroExt,
    ]);
    let ptr = Val::Global("p".into());
    let mut ops = vec![
        Op::Alloca {
            ty: Ty::Struct(vec![]),
            align: 0,
        },
        Op::Alloca {
            ty: Ty::Int(32),
            align: u32::MAX,
        },
        Op::Load {
            ty: Ty::Int(32),
            ptr: ptr.clone(),
            align: 0,
        },
        Op::Load {
            ty: Ty::Int(32),
            ptr: ptr.clone(),
            align: 8,
        },
        Op::Store {
            arg: arg.clone(),
            ptr: ptr.clone(),
            align: 0,
        },
        Op::Store {
            arg: arg.clone(),
            ptr: ptr.clone(),
            align: 16,
        },
        Op::Binary {
            opcode: "odd\n".into(),
            ty: Ty::Int(32),
            left: Val::Int(i128::MIN),
            right: Val::Int(i128::MAX),
        },
        Op::Compare {
            predicate: "eq".into(),
            ty: Ty::Ptr,
            left: Val::Null,
            right: ptr.clone(),
        },
        Op::Cast {
            opcode: "zext".into(),
            arg: arg.clone(),
            to: Ty::Int(64),
        },
        Op::Gep {
            ty: Ty::Int(8),
            ptr: ptr.clone(),
            indices: vec![],
        },
        Op::Gep {
            ty: Ty::Int(8),
            ptr,
            indices: vec![arg.clone(), arg.clone()],
        },
        Op::Extract {
            arg: arg.clone(),
            index: u32::MAX,
        },
        Op::Insert {
            arg: arg.clone(),
            field: arg.clone(),
            index: 0,
        },
        Op::Freeze(arg.clone()),
        Op::Phi {
            ty: Ty::Struct(vec![]),
            incoming: vec![],
        },
        Op::Phi {
            ty: Ty::Int(32),
            incoming: vec![(Val::Int(0), "a".into()), (Val::Int(1), "b".into())],
        },
        Op::Branch("".into()),
        Op::CondBranch {
            condition: Val::Bool(true),
            yes: "a".into(),
            no: "b".into(),
        },
        Op::Return(Some(arg.clone())),
        Op::Return(None),
        Op::Unreachable,
    ];
    for (index, vararg) in [None, Some(vec![]), Some(vec![Ty::Int(8), Ty::Ptr])]
        .into_iter()
        .enumerate()
    {
        ops.push(Op::Call {
            result: Ty::Void,
            result_attrs: vec![Attr::ZeroExt, Attr::NonLazyBind, Attr::ZeroExt],
            callee: Val::Global("target".into()),
            args: if index == 0 {
                vec![]
            } else {
                vec![arg.clone(), arg.clone()]
            },
            vararg,
        });
    }
    let instructions = ops
        .into_iter()
        .enumerate()
        .map(|(index, op)| Inst::new((index % 2 == 0).then(|| format!("r{index}")), op))
        .collect::<Vec<_>>();
    let mut module = Module {
        triple: "target\"\n".into(),
        fingerprint: "fingerprint\n".into(),
        items: vec![
            Item::NamedType("empty".into(), vec![]),
            Item::NamedType("full".into(), type_cases()),
        ],
    };
    let types = type_cases();
    for (index, value) in value_cases().into_iter().enumerate() {
        module.items.push(Item::Global(Global {
            name: format!("g{index}"),
            linkage: Linkage::External,
            ty: types[index % types.len()].clone(),
            initializer: Some(value),
            constant: index % 2 == 0,
            unnamed_addr: index % 3 == 0,
            alignment: if index % 2 == 0 { 0 } else { u32::MAX },
        }));
    }
    for (index, linkage) in [
        Linkage::External,
        Linkage::AvailableExternally,
        Linkage::Internal,
        Linkage::Private,
    ]
    .into_iter()
    .enumerate()
    {
        module.items.push(Item::Global(Global {
            name: "declaration".into(),
            linkage,
            ty: Ty::Ptr,
            initializer: None,
            constant: index % 2 == 0,
            unnamed_addr: index % 2 != 0,
            alignment: 8,
        }));
        for definition in [false, true] {
            module.items.push(Item::Function(Function {
                name: format!("f{index}"),
                linkage,
                signature: Signature {
                    result: Ty::Int(32),
                    result_attrs: vec![Attr::ZeroExt, Attr::NonLazyBind, Attr::ZeroExt],
                    params: if index % 2 == 0 {
                        vec![]
                    } else {
                        vec![
                            Param {
                                ty: Ty::Int(8),
                                name: Some("".into()),
                                attrs: vec![Attr::ZeroExt, Attr::NonLazyBind, Attr::ZeroExt],
                            },
                            Param::unnamed(Ty::Ptr),
                        ]
                    },
                    variadic: index >= 2,
                },
                attrs: vec![Attr::NonLazyBind, Attr::NonLazyBind],
                blocks: definition.then(|| {
                    vec![
                        Block {
                            name: "empty".into(),
                            instructions: vec![],
                        },
                        Block {
                            name: "entry".into(),
                            instructions: instructions.clone(),
                        },
                    ]
                }),
            }));
        }
    }
    assert_eq!(module.to_text(), original(&module));
    module.items.clear();
    assert_eq!(module.to_text(), original(&module));
}
