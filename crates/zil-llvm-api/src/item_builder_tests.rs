use super::*;

fn function(name: &str) -> Function {
    Function {
        name: name.into(),
        linkage: Linkage::Internal,
        signature: Signature::new(Ty::Void, vec![]),
        attrs: vec![],
        blocks: None,
    }
}

#[test]
fn streamed_fragments_preserve_complete_items_and_helper_order() {
    let declaration = function("foreign");
    let global = Global {
        name: "address".into(),
        linkage: Linkage::Private,
        ty: Ty::Ptr,
        initializer: Some(Val::Global("helper".into())),
        constant: true,
        unnamed_addr: false,
        alignment: 8,
    };
    let instruction = Inst::new(None, Op::Return(None));
    let mut main = function("main");
    main.blocks = Some(vec![Block {
        name: "entry".into(),
        instructions: vec![instruction.clone()],
    }]);
    let mut helper = function("helper");
    helper.blocks = Some(vec![Block {
        name: "helper.entry".into(),
        instructions: vec![instruction.clone()],
    }]);
    let expected = vec![
        Item::NamedType("pair".into(), vec![Ty::Int(32), Ty::Ptr]),
        Item::Global(global.clone()),
        Item::Function(declaration.clone()),
        Item::Function(main),
        Item::Function(helper),
    ];
    let events = vec![
        Event::NamedType("pair".into(), vec![Ty::Int(32), Ty::Ptr]),
        Event::Global(global),
        Event::Declare(declaration),
        Event::Begin(function("main")),
        Event::Label("entry".into()),
        Event::Instruction(instruction.clone()),
        Event::End,
        Event::Begin(function("helper")),
        Event::Label("helper.entry".into()),
        Event::Instruction(instruction),
        Event::End,
    ];
    let mut builder = ItemBuilder::new();
    // An appended fragment can begin or end within a function. The sink must
    // retain the enclosing state without retaining the fragment's event tape.
    builder.append(Code(events[..5].to_vec()));
    for event in events[5..8].iter().cloned() {
        builder.push(event);
    }
    builder.append(Code(events[8..].to_vec()));
    assert_eq!(builder.finish().unwrap(), expected);
    assert_eq!(Code(events).into_items().unwrap(), expected);
}

#[test]
fn malformed_streams_retain_the_first_error_until_finish() {
    let instruction = Event::Instruction(Inst::new(None, Op::Return(None)));
    for (events, expected) in [
        (vec![Event::Label("entry".into())], "block outside function"),
        (vec![instruction.clone()], "instruction outside block"),
        (
            vec![Event::Begin(function("main")), instruction],
            "instruction outside block",
        ),
        (
            vec![Event::End, Event::Label("later".into())],
            "unmatched function end",
        ),
        (
            vec![Event::Begin(function("main"))],
            "unterminated function",
        ),
        (
            vec![
                Event::Begin(function("main")),
                Event::Begin(function("nested")),
                Event::End,
            ],
            "malformed lowering event structure",
        ),
        (
            vec![
                Event::Begin(function("main")),
                Event::Declare(function("foreign")),
            ],
            "malformed lowering event structure",
        ),
    ] {
        let mut builder = ItemBuilder::new();
        for event in events.iter().cloned() {
            builder.push(event);
        }
        assert_eq!(builder.finish().unwrap_err(), expected);
        assert_eq!(Code(events).into_items().unwrap_err(), expected);
    }
}
