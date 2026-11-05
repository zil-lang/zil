//! A nominal root may receive one life from the real host and another from
//! the member. Both have ordinal zero in their own distinct declarations.
use super::*;

fn multiowner_fixture() -> Fixture {
    fixture(
        "struct Pair<life a,life b>{n:i32} struct Host<life h>{fn make<life m>()->Pair<'h,'m>{return Pair::<'h,'m>{n:42}}} fn query<life a,life b>(p:&'a i32)->i32{let value:Pair<'a,'b>=#(Host::<'a>.make::<'b>());return value.n}",
        2,
    )
}
#[test]
fn real_host_and_member_lives_share_one_nominal_root() {
    let mut f = multiowner_fixture();
    f.context
        .extend_calls(
            &f.facts.calls,
            &f.facts.scopes,
            &f.raw,
            &f.index,
            &mut f.budget,
        )
        .unwrap();
    f.context
        .validate_type_contexts(&f.facts, &f.raw, &f.index, &mut f.budget)
        .unwrap();
    let binding = f
        .facts
        .bindings
        .iter()
        .find(|binding| binding.sources.is_some())
        .unwrap();
    let sources = binding.sources.as_ref().unwrap();
    assert_eq!(sources.len(), 2);
    assert_eq!(sources[0].parameter, 0);
    assert_eq!(sources[1].parameter, 0);
    assert_ne!(sources[0].scope, sources[1].scope);
    let invocation = Arc::new(f.context);
    let value = crate::ct_artifact::execute::value_context::ValueContext::capture(
        &f.raw,
        &f.facts.bindings,
        Some(invocation),
        &mut f.budget,
    )
    .unwrap();
    let (_, scopes) = value.type_with_scopes(&value.ty, &mut f.budget).unwrap();
    assert!(scopes.iter().any(|scope| scope.parameters == [0, 1]));
}
#[test]
fn real_host_member_source_rejects_same_ordinal_different_owner_substitution() {
    let mut f = multiowner_fixture();
    f.context
        .extend_calls(
            &f.facts.calls,
            &f.facts.scopes,
            &f.raw,
            &f.index,
            &mut f.budget,
        )
        .unwrap();
    let binding = f
        .facts
        .bindings
        .iter_mut()
        .find(|binding| binding.sources.is_some())
        .unwrap();
    let source = binding.sources.as_ref().unwrap()[0];
    binding.sources.as_mut().unwrap()[1] = source;
    binding.parameters[1] = source.parameter;
    let graph = if binding.enumeration {
        f.raw.enum_schemas[binding.schema as usize]
            .life_context
            .as_mut()
            .unwrap()
    } else {
        f.raw.record_schemas[binding.schema as usize]
            .life_context
            .as_mut()
            .unwrap()
    };
    let root = graph.root;
    graph
        .ambient_classes
        .iter_mut()
        .find(|row| row.owner == root && row.class == 1)
        .unwrap()
        .source = source;
    assert!(
        f.context
            .validate_type_contexts(&f.facts, &f.raw, &f.index, &mut f.budget)
            .is_err()
    );
}
