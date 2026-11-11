use super::*;

fn definition(kind: CallableKind, captures: Vec<Type>) -> ClosureDefinition {
    ClosureDefinition {
        storage_life: None,
        id: ClosureId(0),
        function: FunctionId(0),
        captures,
        parameters: vec![],
        result: Type::I64,
        kind,
        lending_captures: vec![],
    }
}
fn byte() -> Type {
    Type::Managed(Box::new(Type::ByteBuffer))
}
fn state() -> Type {
    Type::Package {
        name: "State".into(),
        fields: vec![
            ("left".into(), byte()),
            ("sum".into(), Type::I64),
            ("right".into(), byte()),
        ],
    }
}
fn lower(definition: ClosureDefinition) -> m::Function {
    let mut items = drop_functions(
        &[definition],
        &[ResourceDefinition {
            name: "buffer".into(),
            payload: Type::ByteBuffer,
            teardown: None,
        }],
    )
    .unwrap()
    .into_items()
    .unwrap();
    assert_eq!(items.len(), 1);
    let m::Item::Function(function) = items.pop().unwrap() else {
        unreachable!()
    };
    function
}
fn effects(function: &m::Function) -> Vec<m::Inst> {
    function
        .blocks
        .iter()
        .flatten()
        .flat_map(|b| &b.instructions)
        .filter(|i| {
            !matches!(
                i.op,
                m::Op::Load { .. } | m::Op::CondBranch { .. } | m::Op::Branch(_)
            )
        })
        .cloned()
        .collect()
}

#[test]
fn repeatable_complete_byte_drop_preserves_every_surrender_and_teardown() {
    for captures in [
        vec![byte()],
        vec![state()],
        vec![byte(), Type::I64, state()],
    ] {
        let dynamic = lower(definition(CallableKind::FnOnce, captures.clone()));
        let dynamic_instructions = dynamic
            .blocks
            .iter()
            .flatten()
            .flat_map(|b| &b.instructions)
            .collect::<Vec<_>>();
        assert!(dynamic_instructions.iter().any(|i| matches!(
            i.op,
            m::Op::Load {
                ty: m::Ty::Int(1),
                ..
            }
        )));
        assert!(
            dynamic_instructions
                .iter()
                .any(|i| matches!(i.op, m::Op::CondBranch { .. }))
        );
        for kind in [CallableKind::Fn, CallableKind::FnMut] {
            let static_drop = lower(definition(kind, captures.clone()));
            assert_eq!(static_drop.signature, dynamic.signature);
            assert_eq!(static_drop.name, dynamic.name);
            assert_eq!(static_drop.linkage, dynamic.linkage);
            assert!(static_drop.attrs.is_empty());
            assert_eq!(static_drop.blocks.as_ref().unwrap().len(), 1);
            assert_eq!(
                effects(&static_drop),
                effects(&dynamic),
                "only redundant live reads and control edges may disappear"
            );
            let instructions = &static_drop.blocks.as_ref().unwrap()[0].instructions;
            assert!(
                !instructions
                    .iter()
                    .any(|i| matches!(i.op, m::Op::Load { .. } | m::Op::CondBranch { .. }))
            );
            let mut surrendered = 0;
            let mut cleaned = 0;
            for i in instructions {
                match &i.op {
                    m::Op::Store { arg, .. } => {
                        assert_eq!(arg.value, m::Val::Bool(false));
                        surrendered += 1;
                    }
                    m::Op::Call {
                        callee: m::Val::Global(name),
                        ..
                    } => {
                        assert_eq!(name, "zil.bytes.drop");
                        cleaned += 1;
                        assert!(surrendered >= cleaned);
                    }
                    _ => {}
                }
            }
            assert!(cleaned > 0);
        }
    }
}

#[test]
fn consuming_nested_and_custom_resources_do_not_gain_complete_drop_authority() {
    assert!(!complete_byte_environment(&definition(
        CallableKind::FnOnce,
        vec![byte()]
    )));
    let nested = Type::Package {
        name: "Outer".into(),
        fields: vec![("child".into(), state())],
    };
    let nested_closure = Type::Closure(ClosureId(1));
    let custom = Type::Managed(Box::new(Type::I64));
    for ty in [nested, nested_closure, custom] {
        assert!(!complete_byte_environment(&definition(
            CallableKind::Fn,
            vec![ty.clone()]
        )));
        assert!(!complete_byte_environment(&definition(
            CallableKind::FnMut,
            vec![ty]
        )));
    }
    assert!(complete_byte_environment(&definition(
        CallableKind::FnMut,
        vec![state()]
    )));
    assert!(complete_byte_environment(&definition(
        CallableKind::Fn,
        vec![Type::I64]
    )));
}
