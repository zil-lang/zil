use super::*;

fn closure(id: u32, owning: bool) -> ClosureDefinition {
    ClosureDefinition {
        storage_life: None,
        id: ClosureId(id),
        function: FunctionId(id),
        captures: if owning {
            vec![Type::Managed(Box::new(Type::ByteBuffer))]
        } else {
            vec![Type::I64]
        },
        parameters: vec![],
        result: Type::Unit,
        kind: CallableKind::FnMut,
        lending_captures: vec![],
    }
}
fn functions(definitions: &[ClosureDefinition]) -> Vec<m::Function> {
    let resources = [ResourceDefinition {
        name: "bytes".into(),
        payload: Type::ByteBuffer,
        teardown: None,
    }];
    drop_functions(definitions, &resources)
        .unwrap()
        .into_items()
        .unwrap()
        .into_iter()
        .map(|item| match item {
            m::Item::Function(f) => f,
            _ => panic!("destructor emitter must only emit functions"),
        })
        .collect()
}

#[test]
fn destructor_boundary_threshold_keeps_every_original_instruction_and_identity() {
    for count in [1, 8, 15, 16, 24, 32, 64] {
        let definitions = (0..count).map(|id| closure(id, true)).collect::<Vec<_>>();
        for (id, mut actual) in functions(&definitions).into_iter().enumerate() {
            let original = functions(&[definitions[id].clone()]).pop().unwrap();
            assert_eq!(actual.linkage, m::Linkage::Internal);
            assert_eq!(
                actual.attrs,
                if count >= 16 {
                    vec![m::Attr::NoInline]
                } else {
                    vec![]
                }
            );
            actual.attrs.clear();
            assert_eq!(
                actual, original,
                "only the optimizer's function boundary may change"
            );
        }
    }
}

#[test]
fn destructor_boundary_does_not_classify_nonowning_environments_as_resource_work() {
    let mut definitions = (0..100).map(|id| closure(id, false)).collect::<Vec<_>>();
    assert!(functions(&definitions).iter().all(|f| f.attrs.is_empty()));
    definitions.extend((100..115).map(|id| closure(id, true)));
    assert!(functions(&definitions).iter().all(|f| f.attrs.is_empty()));
    definitions.push(closure(115, true));
    let actual = functions(&definitions);
    assert!(actual[..100].iter().all(|f| f.attrs.is_empty()));
    assert!(actual[100..].iter().all(|f| f.attrs == [m::Attr::NoInline]));
}
