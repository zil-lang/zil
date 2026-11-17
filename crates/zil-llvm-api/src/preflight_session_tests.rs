use super::*;
fn function(name: &str) -> Function {
    Function {
        name: name.into(),
        linkage: Linkage::External,
        signature: Signature::new(Ty::Int(32), vec![]),
        attrs: vec![],
        blocks: Some(
            vec![Block {
                name: "entry".into(),
                instructions: vec![
                    Inst::new(
                        Some("r".into()),
                        Op::Call {
                            result: Ty::Int(32),
                            result_attrs: vec![],
                            callee: Val::Global("callee".into()),
                            args: vec![],
                            vararg: None,
                        },
                    ),
                    Inst::new(
                        None,
                        Op::Return(Some(Arg::new(Ty::Int(32), Val::Local("r".into())))),
                    ),
                ],
            }]
            .into(),
        ),
    }
}
fn module() -> Module {
    let mut callee = function("callee");
    callee.blocks = None;
    Module {
        triple: "x86_64-unknown-linux-gnu".into(),
        fingerprint: "diagnostic".into(),
        items: vec![Item::Function(callee), Item::Function(function("caller"))].into(),
    }
}
fn same_error(session: &mut PreflightSession, module: &Module) {
    assert_eq!(
        session.validate(module).unwrap_err().to_string(),
        crate::validate_module(module).unwrap_err().to_string()
    );
}
#[test]
fn body_reuse_crosses_current_world_extension_and_internalization_but_not_cow_edit() {
    let first = module();
    let mut session = PreflightSession::default();
    session.validate(&first).unwrap();
    assert_eq!(session.body_counts(), (1, 0));
    let mut next = first.clone();
    next.fingerprint = "new-current".into();
    let Item::Function(f) = &mut next.items[1] else {
        panic!()
    };
    f.name = "renamed".into();
    f.linkage = Linkage::Internal;
    let mut unrelated = function("unrelated");
    unrelated.blocks = None;
    next.items.push(Item::Function(unrelated));
    session.validate(&next).unwrap();
    assert_eq!(session.body_counts(), (1, 1));
    let Item::Function(f) = &mut next.items[1] else {
        panic!()
    };
    f.blocks.as_mut().unwrap()[0].instructions[1] =
        Inst::new(None, Op::Return(Some(Arg::new(Ty::Int(64), Val::Int(0)))));
    same_error(&mut session, &next);
    assert_eq!(session.body_counts(), (1, 1));
    session.validate(&first).unwrap();
    assert_eq!(session.body_counts(), (1, 2));
}
#[test]
fn every_current_callee_abi_and_symbol_kind_is_bound_and_first_error_preserved() {
    let first = module();
    let mut session = PreflightSession::default();
    session.validate(&first).unwrap();
    for case in 0..5 {
        let mut next = first.clone();
        match case {
            0 => {
                let Item::Function(f) = &mut next.items[0] else {
                    panic!()
                };
                f.signature.result = Ty::Int(64);
            }
            1 => {
                let Item::Function(f) = &mut next.items[0] else {
                    panic!()
                };
                f.signature.variadic = true;
            }
            2 => {
                next.items.remove(0);
            }
            3 => {
                next.items[0] = Item::Global(Global {
                    name: "callee".into(),
                    linkage: Linkage::External,
                    constant: false,
                    unnamed_addr: false,
                    ty: Ty::Int(32),
                    initializer: None,
                    alignment: 4,
                });
            }
            4 => {
                let duplicate = next.items[0].clone();
                next.items.push(duplicate);
            }
            _ => unreachable!(),
        }
        same_error(&mut session, &next);
    }
    assert_eq!(session.body_counts(), (1, 0));
}
#[test]
fn all_current_headers_and_dead_bodies_still_check_and_failure_publishes_no_body() {
    let first = module();
    let mut broken = first.clone();
    let mut dead = function("dead");
    dead.linkage = Linkage::Internal;
    dead.blocks.as_mut().unwrap()[0].instructions[1] =
        Inst::new(None, Op::Return(Some(Arg::new(Ty::Int(64), Val::Int(0)))));
    broken.items.push(Item::Function(dead));
    let mut session = PreflightSession::default();
    same_error(&mut session, &broken);
    assert!(session.bodies.is_empty());
    session.validate(&first).unwrap();
    assert_eq!(session.body_counts().1, 0);
    let mut headers = first.clone();
    let Item::Function(f) = &mut headers.items[1] else {
        panic!()
    };
    f.attrs = vec![Attr::NoInline, Attr::NoInline];
    same_error(&mut session, &headers);
    let mut target = first.clone();
    target.triple = "unsupported".into();
    same_error(&mut session, &target);
}
#[test]
fn named_layout_change_invalidates_even_same_body_and_missing_address_fails() {
    let mut first = module();
    let Item::Function(f) = &mut first.items[1] else {
        panic!()
    };
    f.blocks.as_mut().unwrap()[0].instructions.insert(
        0,
        Inst::new(
            Some("layout".into()),
            Op::Extract {
                arg: Arg::new(Ty::Named("pair".into()), Val::Zero),
                index: 1,
            },
        ),
    );
    first.items.insert(
        0,
        Item::NamedType("pair".into(), vec![Ty::Int(32), Ty::Int(32)]),
    );
    let mut session = PreflightSession::default();
    session.validate(&first).unwrap();
    let mut changed = first.clone();
    let Item::NamedType(_, fields) = &mut changed.items[0] else {
        panic!()
    };
    fields.pop();
    same_error(&mut session, &changed);
    let mut first = module();
    let Item::Function(f) = &mut first.items[1] else {
        panic!()
    };
    f.blocks.as_mut().unwrap()[0].instructions.insert(
        0,
        Inst::new(
            Some("address".into()),
            Op::Freeze(Arg::new(Ty::Ptr, Val::Global("data".into()))),
        ),
    );
    first.items.push(Item::Global(Global {
        name: "data".into(),
        linkage: Linkage::External,
        constant: false,
        unnamed_addr: false,
        ty: Ty::Int(32),
        initializer: None,
        alignment: 4,
    }));
    let mut session = PreflightSession::default();
    session.validate(&first).unwrap();
    let mut changed = first.clone();
    changed.items.pop();
    same_error(&mut session, &changed);
}

#[test]
fn current_declaration_to_definition_names_preserve_callee_abi_reuse() {
    let mut first = module();
    let Item::Function(callee) = &mut first.items[0] else {
        panic!()
    };
    callee.signature.params = vec![Param::unnamed(Ty::Int(32))];
    let Item::Function(caller) = &mut first.items[1] else {
        panic!()
    };
    let Op::Call { args, .. } = &mut caller.blocks.as_mut().unwrap()[0].instructions[0].op else {
        panic!()
    };
    args.push(Arg::new(Ty::Int(32), Val::Int(7)));
    let mut session = PreflightSession::default();
    session.validate(&first).unwrap();
    let mut next = first.clone();
    let Item::Function(callee) = &mut next.items[0] else {
        panic!()
    };
    callee.signature.params[0].name = Some("actual_argument".into());
    callee.blocks = Some(
        vec![Block {
            name: "entry".into(),
            instructions: vec![Inst::new(
                None,
                Op::Return(Some(Arg::new(
                    Ty::Int(32),
                    Val::Local("actual_argument".into()),
                ))),
            )],
        }]
        .into(),
    );
    session.validate(&next).unwrap();
    assert_eq!(session.body_counts(), (2, 1));
    let mut bad = next.clone();
    let Item::Function(callee) = &mut bad.items[0] else {
        panic!()
    };
    callee.signature.params[0].name = Some("wrong_own_binding".into());
    same_error(&mut session, &bad);
}
#[test]
fn later_malformed_callee_type_never_clones_or_publishes_a_partial_receipt() {
    let mut first = module();
    let Item::Function(caller) = &mut first.items[1] else {
        panic!()
    };
    caller.blocks.as_mut().unwrap()[0].instructions[0] = Inst::new(
        Some("ptr".into()),
        Op::Freeze(Arg::new(Ty::Ptr, Val::Global("callee".into()))),
    );
    caller.blocks.as_mut().unwrap()[0].instructions[1] =
        Inst::new(None, Op::Return(Some(Arg::new(Ty::Int(32), Val::Int(7)))));
    let Item::Function(callee) = &mut first.items[0] else {
        panic!()
    };
    let mut ty = Ty::Int(32);
    for _ in 0..140 {
        ty = Ty::Struct(vec![ty]);
    }
    callee.signature.result = ty;
    first.items.swap(0, 1);
    let mut session = PreflightSession::default();
    same_error(&mut session, &first);
    assert!(session.bodies.is_empty());
    assert_eq!(session.cost, 0);
}
