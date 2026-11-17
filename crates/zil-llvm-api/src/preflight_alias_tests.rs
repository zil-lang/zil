//! Borrowed current names must check the same complete world as flat inputs.
use super::*;
use std::cell::Cell;

const TRIPLE: &str = "x86_64-unknown-linux-gnu";

fn function(name: &str) -> Function {
    Function {
        name: name.into(),
        linkage: Linkage::External,
        signature: Signature::new(Ty::Int(32), vec![]),
        attrs: vec![],
        blocks: Some(
            vec![Block {
                name: "entry".into(),
                instructions: vec![Inst::new(
                    None,
                    Op::Return(Some(Arg::new(Ty::Int(32), Val::Int(7)))),
                )],
            }]
            .into(),
        ),
    }
}

fn caller(name: &str) -> Function {
    let mut f = function(name);
    f.blocks.as_mut().unwrap()[0].instructions = vec![
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
    ];
    f
}

fn global() -> Item {
    Item::Global(Global {
        name: "data".into(),
        linkage: Linkage::External,
        ty: Ty::Ptr,
        initializer: Some(Val::Global("first".into())),
        constant: true,
        unnamed_addr: false,
        alignment: 8,
    })
}

// Materialize names only in the differential oracle, never in production.
fn flat(items: &[(&str, &Item)]) -> Module {
    Module {
        triple: TRIPLE.into(),
        fingerprint: "alias-oracle".into(),
        items: items
            .iter()
            .map(|&(name, item)| {
                let mut item = item.clone();
                if let Item::Function(f) = &mut item {
                    f.name = name.into();
                }
                item
            })
            .collect(),
    }
}

fn rejected(session: &mut PreflightSession, items: &[(&str, &Item)], expected: &str) {
    let module = flat(items);
    assert_eq!(validate_impl::<true>(&module).unwrap_err(), expected);
    assert_eq!(validate_impl::<false>(&module).unwrap_err(), expected);
    let error = session.validate_aliases(TRIPLE, items).unwrap_err();
    assert_eq!(error.kind, crate::ErrorKind::Verify);
    assert_eq!(error.message, expected);
    assert_eq!(session.validate(&module).unwrap_err().message, expected);
}

#[test]
fn complete_alias_world_matches_flat_checks_and_schedules() {
    let record = Item::NamedType("Record".into(), vec![Ty::Int(32)]);
    let data = global();
    let declaration = Item::Function(function("another_template_name").declaration());
    let mut f = caller("template name is not a current symbol");
    f.blocks.as_mut().unwrap()[0].instructions.splice(
        0..0,
        [
            Inst::new(
                Some("address".into()),
                Op::Freeze(Arg::new(Ty::Ptr, Val::Global("data".into()))),
            ),
            Inst::new(
                Some("field".into()),
                Op::Extract {
                    arg: Arg::new(Ty::Named("Record".into()), Val::Zero),
                    index: 0,
                },
            ),
        ],
    );
    let template = Item::Function(f);
    let items = [
        ("first", &template),
        ("second", &template),
        ("callee", &declaration),
        ("other_declaration", &declaration),
        ("data", &data),
        ("Record", &record),
    ];
    let module = flat(&items);
    let expected = validate_impl::<true>(&module).unwrap().functions;
    let before = BODY_VALIDATIONS.with(Cell::get);
    let actual = validate_inputs_cached::<true>(TRIPLE, &items, None).unwrap();
    assert_eq!(BODY_VALIDATIONS.with(Cell::get) - before, 1);
    assert_eq!(actual.functions, expected);
    assert_eq!(actual.functions["first"], actual.functions["second"]);
    assert!(actual.functions["callee"].is_empty());
    assert!(actual.functions["other_declaration"].is_empty());
    validate_impl::<false>(&module).unwrap();
    let before = BODY_VALIDATIONS.with(Cell::get);
    PreflightSession::default()
        .validate_aliases(TRIPLE, &items)
        .unwrap();
    assert_eq!(BODY_VALIDATIONS.with(Cell::get) - before, 1);
}

#[test]
fn duplicate_invalid_and_dead_body_errors_match_flat_in_original_order() {
    let good = Item::Function(function("template"));
    let mut bad = function("old_dead_name");
    bad.linkage = Linkage::Internal;
    bad.blocks.as_mut().unwrap()[0].instructions[0] = Inst::new(
        None,
        Op::Return(Some(Arg::new(Ty::Int(32), Val::Bool(true)))),
    );
    let bad = Item::Function(bad);
    let mut session = PreflightSession::default();
    session
        .validate_aliases(TRIPLE, &[("first", &good)])
        .unwrap();
    // Inventory precedes all bodies, including an earlier malformed dead body.
    rejected(
        &mut session,
        &[("dead", &bad), ("same", &good), ("same", &good)],
        "duplicate/invalid global symbol same",
    );
    for name in ["", "123", "bad name"] {
        rejected(
            &mut session,
            &[("dead", &bad), (name, &good)],
            &format!("duplicate/invalid global symbol {name}"),
        );
    }
    rejected(
        &mut session,
        &[("first", &good), ("actual_dead", &bad)],
        "@actual_dead / entry / 0: boolean constant is not i1",
    );
}

#[test]
fn shared_template_checks_once_and_session_reuses_both_input_views() {
    let template = Item::Function(function("stored"));
    let items = [("first", &template), ("second", &template)];
    let mut session = PreflightSession::default();
    let before = BODY_VALIDATIONS.with(Cell::get);
    session.validate_aliases(TRIPLE, &items).unwrap();
    assert_eq!(BODY_VALIDATIONS.with(Cell::get) - before, 1);
    assert_eq!(session.body_counts(), (1, 0));
    // The ordinary adapter preserves the shared body allocation and names.
    let module = flat(&items);
    let before = BODY_VALIDATIONS.with(Cell::get);
    session.validate(&module).unwrap();
    assert_eq!(BODY_VALIDATIONS.with(Cell::get), before);
    assert_eq!(session.body_counts(), (1, 1));
    session
        .validate_aliases(TRIPLE, &[("third", &template), ("fourth", &template)])
        .unwrap();
    assert_eq!(session.body_counts(), (1, 2));
    let mut fresh = PreflightSession::default();
    fresh.validate_aliases(TRIPLE, &items).unwrap();
    assert_eq!(fresh.body_counts(), (1, 0));
}

#[test]
fn alias_callee_abi_symbol_kind_and_binding_changes_reject_prior_success() {
    let template = Item::Function(caller("stored_caller"));
    let callee = Item::Function(function("stored_callee").declaration());
    let mut session = PreflightSession::default();
    session
        .validate_aliases(TRIPLE, &[("first", &template), ("callee", &callee)])
        .unwrap();
    assert_eq!(session.body_counts(), (1, 0));
    for case in 0..4 {
        let mut changed = callee.clone();
        let Item::Function(f) = &mut changed else {
            unreachable!()
        };
        match case {
            0 => f.signature.result = Ty::Int(64),
            1 => f.signature.variadic = true,
            2 => f.signature.result_attrs = vec![Attr::SignExt],
            3 => f.signature.params.push(Param::unnamed(Ty::Int(32))),
            _ => unreachable!(),
        }
        rejected(
            &mut session,
            &[("actual_caller", &template), ("callee", &changed)],
            if case == 1 {
                "@actual_caller / entry / 0: variadic call signature missing/mismatched"
            } else {
                "@actual_caller / entry / 0: call signature mismatch for @callee"
            },
        );
    }
    rejected(
        &mut session,
        &[("actual_caller", &template), ("renamed_callee", &callee)],
        "@actual_caller / entry / 0: undefined symbol @callee",
    );
    let data = Item::Global(Global {
        name: "callee".into(),
        linkage: Linkage::External,
        ty: Ty::Int(32),
        initializer: None,
        constant: false,
        unnamed_addr: false,
        alignment: 4,
    });
    rejected(
        &mut session,
        &[("actual_caller", &template), ("callee", &data)],
        "@actual_caller / entry / 0: calling a global that is not a function",
    );
    assert_eq!(session.body_counts(), (1, 0));
}

#[test]
fn current_headers_and_body_edits_keep_first_error_and_do_not_publish_failure() {
    let template = Item::Function(function("stored"));
    let mut header = function("stored_header");
    header.attrs = vec![Attr::NoInline, Attr::NoInline];
    let header = Item::Function(header);
    let mut body = function("stored_body");
    body.signature.result = Ty::Int(64);
    let body = Item::Function(body);
    let mut session = PreflightSession::default();
    rejected(
        &mut session,
        &[
            ("first", &template),
            ("bad_body", &body),
            ("bad_header", &header),
        ],
        "@bad_body / entry / 0: return type mismatch",
    );
    rejected(
        &mut session,
        &[
            ("first", &template),
            ("bad_header", &header),
            ("bad_body", &body),
        ],
        "duplicate machine ABI attribute",
    );
    // The successful first body in either failed world was never published.
    let before = BODY_VALIDATIONS.with(Cell::get);
    session
        .validate_aliases(TRIPLE, &[("first", &template)])
        .unwrap();
    assert_eq!(BODY_VALIDATIONS.with(Cell::get) - before, 1);
    let mut changed = template.clone();
    let Item::Function(f) = &mut changed else {
        unreachable!()
    };
    f.blocks.as_mut().unwrap()[0].instructions[0] = Inst::new(
        None,
        Op::Return(Some(Arg::new(Ty::Int(32), Val::Bool(true)))),
    );
    rejected(
        &mut session,
        &[("actual_edit", &changed)],
        "@actual_edit / entry / 0: boolean constant is not i1",
    );
}

#[test]
fn globals_and_named_types_cannot_hide_input_names_with_aliases() {
    let data = global();
    let record = Item::NamedType("Record".into(), vec![Ty::Int(32)]);
    let mut session = PreflightSession::default();
    for name in ["hidden", ""] {
        assert_eq!(
            session
                .validate_aliases(TRIPLE, &[(name, &data)])
                .unwrap_err()
                .message,
            "global alias does not match item name",
        );
        assert_eq!(
            session
                .validate_aliases(TRIPLE, &[(name, &record)])
                .unwrap_err()
                .message,
            "named type alias does not match item name",
        );
    }
    for item in [
        Item::NamedType("bad type".into(), vec![Ty::Int(32)]),
        Item::Global(Global {
            name: "bad global".into(),
            linkage: Linkage::External,
            ty: Ty::Int(32),
            initializer: None,
            constant: false,
            unnamed_addr: false,
            alignment: 4,
        }),
    ] {
        assert!(
            session
                .validate_aliases(TRIPLE, &[("valid", &item)])
                .is_err()
        );
    }
    let first = Item::Function(function("stored"));
    rejected(
        &mut session,
        &[("data", &first), ("data", &data)],
        "duplicate/invalid global symbol data",
    );
    rejected(
        &mut session,
        &[("Record", &record), ("Record", &record)],
        "duplicate/invalid type name",
    );
}
