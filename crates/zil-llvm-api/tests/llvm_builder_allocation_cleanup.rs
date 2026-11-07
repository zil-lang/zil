//! Allocation changes must preserve event, operand and dependency order.
mod support210;
use support210::*;
use zil_llvm_api::{model::*, validate_module};

#[test]
fn bounded_fragments_match_one_event_stream_and_manual_items() {
    let mut f = f("first", vec![], Ty::Int(32), vec![]);
    f.blocks = None;
    let body = vec![i("", Op::Return(Some(a(Ty::Int(32), 42))))];
    let code = Code(vec![
        Event::Begin(f.clone()),
        Event::Label("entry".into()),
        Event::Instruction(body[0].clone()),
        Event::End,
    ]);
    let mut expected = f;
    expected.blocks = Some(vec![b("entry", body)]);
    assert_eq!(
        code.clone().into_items().unwrap(),
        vec![Item::Function(expected)]
    );
    let mut second = code.clone();
    if let Event::Begin(f) = &mut second.0[0] {
        f.name = "second".into();
    }
    let mut together = code.clone();
    together.append(second.clone());
    let mut separate = code.into_items().unwrap();
    separate.extend(second.into_items().unwrap());
    let module =
        Module::from_code("x86_64-unknown-linux-gnu".into(), "test".into(), together).unwrap();
    assert_eq!(module.items, separate);
    validate_module(&module).unwrap();
}

#[test]
fn fragments_do_not_hide_missing_or_nested_function_boundaries() {
    let mut f = f("f", vec![], Ty::Void, vec![]);
    f.blocks = None;
    for events in [
        vec![Event::Begin(f.clone())],
        vec![Event::End],
        vec![Event::Label("orphan".into())],
        vec![Event::Instruction(i("", Op::Return(None)))],
        vec![Event::Begin(f.clone()), Event::Begin(f), Event::End],
    ] {
        assert!(Code(events).into_items().is_err());
    }
}

#[test]
fn operand_iterator_keeps_order_multiplicity_and_nested_values() {
    let x = Val::Local("x".into());
    let y = Val::Local("y".into());
    let address = Val::Gep {
        ty: Ty::Int(8),
        ptr: Box::new(Val::Global("storage".into())),
        indices: vec![a(Ty::Int(64), x.clone())],
    };
    let cases = vec![
        (
            Op::Store {
                arg: a(Ty::Int(32), x.clone()),
                ptr: y.clone(),
                align: 4,
            },
            vec![&x, &y],
        ),
        (
            Op::Binary {
                opcode: "add".into(),
                ty: Ty::Int(32),
                left: x.clone(),
                right: x.clone(),
            },
            vec![&x, &x],
        ),
        (
            Op::Gep {
                ty: Ty::Int(8),
                ptr: address.clone(),
                indices: vec![a(Ty::Int(64), y.clone())],
            },
            vec![&address, &y],
        ),
        (
            Op::Call {
                result: Ty::Void,
                result_attrs: vec![],
                callee: x.clone(),
                args: vec![a(Ty::Ptr, address.clone()), a(Ty::Int(32), y.clone())],
                vararg: None,
            },
            vec![&x, &address, &y],
        ),
        (
            Op::Phi {
                ty: Ty::Int(32),
                incoming: vec![(x.clone(), "a".into()), (y.clone(), "b".into())],
            },
            vec![&x, &y],
        ),
        (Op::Return(None), vec![]),
    ];
    for (op, expected) in cases {
        assert_eq!(op.values().collect::<Vec<_>>(), expected);
    }
    let mut uses = Vec::new();
    address.local_uses(&mut uses);
    assert_eq!(uses, ["x"]);
}

#[test]
fn dense_schedule_deduplicates_operand_and_sequence_edges_without_missing_errors() {
    let mut instructions = vec![i("v0", Op::Freeze(a(Ty::Int(32), 42)))];
    for n in 1..256 {
        instructions.push(i(
            &format!("v{n}"),
            Op::Binary {
                opcode: "xor".into(),
                ty: Ty::Int(32),
                left: Val::Local(format!("v{}", n - 1)),
                right: Val::Local(format!("v{}", n - 1)),
            },
        ));
    }
    instructions.push(i("", Op::Return(Some(a(Ty::Int(32), "%v255")))));
    let mut module = m(vec![Item::Function(f(
        "main",
        vec![],
        Ty::Int(32),
        vec![b("entry", instructions)],
    ))]);
    validate_module(&module).unwrap();
    let Item::Function(f) = &mut module.items[0] else {
        unreachable!()
    };
    let Op::Binary { left, .. } = &mut f.blocks.as_mut().unwrap()[0].instructions[1].op else {
        unreachable!()
    };
    *left = Val::Local("v255".into());
    assert!(
        validate_module(&module)
            .unwrap_err()
            .message
            .contains("does not dominate")
    );
}
