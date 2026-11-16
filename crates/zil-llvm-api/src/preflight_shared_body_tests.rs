//! Request-local shared-body reuse must preserve current-input validation.
use super::*;
use std::cell::Cell;

fn inst(name: Option<&str>, op: Op) -> Inst {
    Inst::new(name.map(str::to_owned), op)
}

fn ret(value: Val) -> Inst {
    inst(None, Op::Return(Some(Arg::new(Ty::Int(32), value))))
}

fn function(name: &str) -> Function {
    Function {
        name: name.into(),
        linkage: Linkage::External,
        signature: Signature::new(Ty::Int(32), vec![Param::new(Ty::Int(32), "input")]),
        attrs: vec![],
        blocks: Some(
            vec![Block {
                name: "entry".into(),
                instructions: vec![
                    inst(
                        Some("value"),
                        Op::Freeze(Arg::new(Ty::Int(32), Val::Local("input".into()))),
                    ),
                    ret(Val::Local("value".into())),
                ],
            }]
            .into(),
        ),
    }
}

fn module(items: Vec<Item>) -> Module {
    Module {
        triple: "x86_64-unknown-linux-gnu".into(),
        fingerprint: "shared-body-ci".into(),
        items: items.into(),
    }
}

fn renamed(original: &Function, name: &str) -> Function {
    let mut copy = original.clone();
    copy.name = name.into();
    copy
}

fn fresh_bodies(original: &Module) -> Module {
    let mut fresh = original.clone();
    for item in fresh.items.iter_mut() {
        if let Item::Function(f) = item
            && let Some(blocks) = &f.blocks
        {
            f.blocks = Some(blocks.iter().cloned().collect());
        }
    }
    fresh
}

fn rejected(current: &Module, expected: &str) {
    let fresh = fresh_bodies(current);
    assert_eq!(validate_impl::<true>(current).unwrap_err(), expected);
    assert_eq!(validate_impl::<false>(current).unwrap_err(), expected);
    assert_eq!(validate_impl::<true>(&fresh).unwrap_err(), expected);
    assert_eq!(validate_impl::<false>(&fresh).unwrap_err(), expected);
}

#[test]
fn shared_success_checks_once_in_both_modes_and_keeps_every_named_schedule() {
    let mut first = function("first");
    first.blocks.as_mut().unwrap()[0].instructions[1] = inst(None, Op::Branch("exit".into()));
    first.blocks.as_mut().unwrap().push(Block {
        name: "exit".into(),
        instructions: vec![
            inst(
                Some("joined"),
                Op::Phi {
                    ty: Ty::Int(32),
                    incoming: vec![(Val::Local("value".into()), "entry".into())],
                },
            ),
            ret(Val::Local("joined".into())),
        ],
    });
    let second = renamed(&first, "second");
    assert!(
        first
            .blocks
            .as_ref()
            .unwrap()
            .ptr_eq(second.blocks.as_ref().unwrap())
    );
    let mut declaration = first.declaration();
    declaration.name = "declaration".into();
    let shared = module(vec![
        Item::Function(first),
        Item::Function(second),
        Item::Function(declaration),
    ]);

    let before = BODY_VALIDATIONS.with(Cell::get);
    let schedules = validate_impl::<true>(&shared).unwrap();
    assert_eq!(BODY_VALIDATIONS.with(Cell::get) - before, 1);
    assert_eq!(schedules.functions.len(), 3);
    assert_eq!(schedules.functions["first"], vec![(0, 0), (1, 1), (0, 1)]);
    assert_eq!(schedules.functions["second"], vec![(0, 0), (1, 1), (0, 1)]);
    assert!(schedules.functions["declaration"].is_empty());
    let before = BODY_VALIDATIONS.with(Cell::get);
    assert!(
        validate_impl::<false>(&shared)
            .unwrap()
            .functions
            .is_empty()
    );
    assert_eq!(
        BODY_VALIDATIONS.with(Cell::get) - before,
        1,
        "new invocation checks again"
    );

    let fresh = fresh_bodies(&shared);
    let (Item::Function(a), Item::Function(b)) = (&fresh.items[0], &fresh.items[1]) else {
        panic!("fixture definitions")
    };
    assert_eq!(a.blocks, b.blocks);
    assert!(
        !a.blocks
            .as_ref()
            .unwrap()
            .ptr_eq(b.blocks.as_ref().unwrap())
    );
    let before = BODY_VALIDATIONS.with(Cell::get);
    assert_eq!(
        validate_impl::<true>(&fresh).unwrap().functions,
        schedules.functions
    );
    assert_eq!(BODY_VALIDATIONS.with(Cell::get) - before, 2);
    let before = BODY_VALIDATIONS.with(Cell::get);
    assert!(validate_impl::<false>(&fresh).unwrap().functions.is_empty());
    assert_eq!(BODY_VALIDATIONS.with(Cell::get) - before, 2);
}

#[test]
fn shared_body_with_changed_signature_matches_fresh_current_validation() {
    let first = function("first");
    for (case, expected) in [
        (0, "@second / entry / 0: undefined SSA value %input"),
        (
            1,
            "@second / entry / 0: SSA type mismatch for %input: i64 != i32",
        ),
        (2, "@second / entry / 1: return type mismatch"),
        (3, "variadic definition not supported by machine lowering"),
        (4, "conflicting integer ABI extension attributes"),
        (5, "conflicting integer ABI extension attributes"),
    ] {
        let mut second = renamed(&first, "second");
        match case {
            0 => second.signature.params[0].name = Some("different".into()),
            1 => second.signature.params[0].ty = Ty::Int(64),
            2 => second.signature.result = Ty::Int(64),
            3 => second.signature.variadic = true,
            4 => second.signature.params[0].attrs = vec![Attr::ZeroExt, Attr::SignExt],
            5 => second.signature.result_attrs = vec![Attr::ZeroExt, Attr::SignExt],
            _ => unreachable!(),
        }
        assert!(
            first
                .blocks
                .as_ref()
                .unwrap()
                .ptr_eq(second.blocks.as_ref().unwrap())
        );
        rejected(
            &module(vec![Item::Function(first.clone()), Item::Function(second)]),
            expected,
        );
    }
    // Valid ABI attribute differences still require distinct exact signatures.
    let mut second = renamed(&first, "second");
    second.signature.params[0].attrs = vec![Attr::ZeroExt];
    let mut third = renamed(&first, "third");
    third.signature.result_attrs = vec![Attr::SignExt];
    let current = module(vec![
        Item::Function(first),
        Item::Function(second),
        Item::Function(third),
    ]);
    let before = BODY_VALIDATIONS.with(Cell::get);
    validate_impl::<true>(&current).unwrap();
    assert_eq!(BODY_VALIDATIONS.with(Cell::get) - before, 3);
    let before = BODY_VALIDATIONS.with(Cell::get);
    validate_impl::<false>(&current).unwrap();
    assert_eq!(BODY_VALIDATIONS.with(Cell::get) - before, 3);
}

#[test]
fn prior_success_does_not_authorize_changed_referenced_world() {
    let mut caller = function("caller");
    caller.blocks.as_mut().unwrap()[0].instructions = vec![
        inst(
            Some("address"),
            Op::Freeze(Arg::new(Ty::Ptr, Val::Global("data".into()))),
        ),
        inst(
            Some("called"),
            Op::Call {
                result: Ty::Int(32),
                result_attrs: vec![],
                callee: Val::Global("callee".into()),
                args: vec![Arg::new(Ty::Int(32), Val::Local("input".into()))],
                vararg: None,
            },
        ),
        inst(
            Some("field"),
            Op::Extract {
                arg: Arg::new(Ty::Named("Record".into()), Val::Zero),
                index: 0,
            },
        ),
        ret(Val::Local("field".into())),
    ];
    let second = renamed(&caller, "second");
    let valid = module(vec![
        Item::NamedType("Record".into(), vec![Ty::Int(32)]),
        Item::Global(Global {
            name: "data".into(),
            linkage: Linkage::External,
            ty: Ty::Int(32),
            initializer: None,
            constant: false,
            unnamed_addr: false,
            alignment: 4,
        }),
        Item::Function(function("callee").declaration()),
        Item::Function(caller),
        Item::Function(second),
    ]);
    validate_impl::<true>(&valid).unwrap();
    validate_impl::<false>(&valid).unwrap();
    for (case, expected) in [
        (0, "@caller / entry / 0: undefined symbol @data"),
        (
            1,
            "@caller / entry / 1: call signature mismatch for @callee",
        ),
        (
            2,
            "@caller / entry / 3: SSA type mismatch for %field: i64 != i32",
        ),
    ] {
        let mut current = valid.clone();
        match case {
            0 => {
                let Item::Global(g) = &mut current.items[1] else {
                    panic!("global")
                };
                g.name = "other_data".into();
            }
            1 => {
                let Item::Function(f) = &mut current.items[2] else {
                    panic!("callee")
                };
                f.signature.result = Ty::Int(64);
            }
            2 => {
                let Item::NamedType(_, fields) = &mut current.items[0] else {
                    panic!("type")
                };
                fields[0] = Ty::Int(64);
            }
            _ => unreachable!(),
        }
        rejected(&current, expected);
    }
}

#[test]
fn cow_body_edit_is_rejected_without_changing_the_shared_original() {
    let first = function("first");
    let mut second = renamed(&first, "second");
    second.blocks.as_mut().unwrap()[0].instructions[0] = inst(
        Some("value"),
        Op::Freeze(Arg::new(Ty::Int(32), Val::Bool(true))),
    );
    assert!(
        !first
            .blocks
            .as_ref()
            .unwrap()
            .ptr_eq(second.blocks.as_ref().unwrap())
    );
    rejected(
        &module(vec![Item::Function(first.clone()), Item::Function(second)]),
        "@second / entry / 0: boolean constant is not i1",
    );
    validate_impl::<true>(&module(vec![Item::Function(first.clone())])).unwrap();
    validate_impl::<false>(&module(vec![Item::Function(first)])).unwrap();
}

#[test]
fn shared_success_keeps_current_headers_and_original_first_error_order() {
    let first = function("first");
    let mut bad_body = renamed(&first, "second");
    bad_body.signature.result = Ty::Int(64);
    let mut bad_header = renamed(&first, "third");
    bad_header.attrs = vec![Attr::ZeroExt];
    rejected(
        &module(vec![
            Item::Function(first.clone()),
            Item::Function(bad_body.clone()),
            Item::Function(bad_header.clone()),
        ]),
        "@second / entry / 1: return type mismatch",
    );
    rejected(
        &module(vec![
            Item::Function(first.clone()),
            Item::Function(bad_header),
            Item::Function(bad_body),
        ]),
        "attribute is invalid for its machine ABI position",
    );
    let mut duplicate = renamed(&first, "first");
    duplicate.signature.result = Ty::Int(64);
    rejected(
        &module(vec![Item::Function(first), Item::Function(duplicate)]),
        "duplicate/invalid global symbol first",
    );
}
