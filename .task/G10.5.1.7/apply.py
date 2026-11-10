#!/usr/bin/env python3
from pathlib import Path
import hashlib
p=Path('crates/zil-backend-llvm/src/emit/resource_closures.rs');s=p.read_text()
assert hashlib.sha1(b'blob '+str(len(s.encode())).encode()+b'\0'+s.encode()).hexdigest()=='e8013b2aeabe931d5926fe693068ce583eeb5b64'
a='''    let mut text = m::Code::new();
    // Every concrete environment has an addressable field cleanup entry.'''
b='''    let mut text = m::Code::new();
    // A large family of owning environments can repeatedly expand teardown
    // state into its users. Keep just those destructor boundaries; ordinary
    // callable bodies and buffer growth retain their original inline policy.
    // This attribute asserts no coldness, aliasing or lifetime facts.
    let bounded_teardown = closures.iter().filter(|c| owns(c)).take(16).count() == 16;
    // Every concrete environment has an addressable field cleanup entry.'''
assert s.count(a)==1;s=s.replace(a,b)
a='''                attrs: vec![],
                blocks: None,
            }),
            m::Event::Label("entry".into()),'''
b='''                attrs: if bounded_teardown && owns(definition) {
                    vec![m::Attr::NoInline]
                } else {
                    vec![]
                },
                blocks: None,
            }),
            m::Event::Label("entry".into()),'''
assert s.count(a)==1;s=s.replace(a,b)
s+='\n#[cfg(test)]\nmod destructor_policy_tests;\n';p.write_text(s)
p=p.parent/'resource_closures/destructor_policy_tests.rs';p.parent.mkdir(exist_ok=True)
p.write_text('''use super::*;

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
            assert_eq!(actual.attrs, if count >= 16 { vec![m::Attr::NoInline] } else { vec![] });
            actual.attrs.clear();
            assert_eq!(actual, original, "only the optimizer's function boundary may change");
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
''')
print('bounded destructor candidate prepared; no main changes')
