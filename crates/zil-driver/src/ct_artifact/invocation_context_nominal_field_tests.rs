//! A real nominal declaration supplies field-local Type/Const/Life bindings.
//! Producer parent indices, claimed aliases and other legitimate fields are
//! independently checked against the frozen declaration and full raw layout.
use super::*;

fn nested_fixture() -> Fixture {
    fixture(
        "struct Leaf<T:Copy,#N:i32,life a>{n:T} struct Parent<T:Copy,#N:i32,life a,life b>{left:Leaf<T,N,'a>,right:Leaf<T,N,'b>} fn make<life a,life b>()->Parent<i32,37,'a,'b>{return Parent::<i32,37,'a,'b>{left:Leaf::<i32,37,'a>{n:41},right:Leaf::<i32,37,'b>{n:42}}} fn forward<life a,life b>()->Parent<i32,37,'a,'b>{return make::<'a,'b>()} fn query<T:Copy,#N:i32,life x,life y>(p:&'x i32)->i32{let value=#(forward::<'x,'y>());return value.left.n}",
        4,
    )
}
fn admit_nested(f: &mut Fixture) {
    f.context
        .extend_calls(
            &f.facts.calls,
            &f.facts.scopes,
            &f.raw,
            &f.index,
            &mut f.budget,
        )
        .unwrap();
}
#[test]
fn original_nominal_fields_preserve_complete_type_const_and_life_actuals_after_wire() {
    let mut f = nested_fixture();
    admit_nested(&mut f);
    let limits = Limits::default();
    f.raw = zil_ir::ct_wire::decode_program(
        &zil_ir::ct_wire::encode_program(&f.raw, &limits).unwrap(),
        &limits,
    )
    .unwrap();
    assert!(
        f.facts
            .bindings
            .iter()
            .filter(|binding| binding.parent.is_some())
            .count()
            >= 2
    );
    f.context
        .validate_type_contexts(&f.facts, &f.raw, &f.index, &mut f.budget)
        .unwrap();
    let value = crate::ct_artifact::execute::value_context::ValueContext::capture(
        &f.raw,
        &f.facts.bindings,
        Some(Arc::new(f.context)),
        &mut f.budget,
    )
    .unwrap();
    let (_, scopes) = value.type_with_scopes(&value.ty, &mut f.budget).unwrap();
    assert!(scopes.iter().any(|scope| scope.parameters == [2, 3]));
}
#[test]
fn original_nominal_field_rejects_parent_site_and_complete_alias_substitutions() {
    for attack in 0..11 {
        let mut f = nested_fixture();
        admit_nested(&mut f);
        let child = f
            .facts
            .bindings
            .iter()
            .position(|binding| binding.parent.is_some())
            .unwrap();
        let parent = f.facts.bindings[child].parent.unwrap();
        let sibling = f
            .facts
            .bindings
            .iter()
            .enumerate()
            .find(|(position, binding)| *position != child && binding.parent == Some(parent))
            .unwrap()
            .0;
        match attack {
            0 => f.facts.bindings[child].parent = None,
            1 => {
                f.facts.bindings[child].parent = Some(compiler::SourceSchemaId::Record(
                    CtRecordSchemaId(f.facts.bindings[child].schema),
                ))
            }
            2 => {
                f.facts.bindings[child].parent =
                    Some(compiler::SourceSchemaId::Record(CtRecordSchemaId(u32::MAX)))
            }
            3 => {
                f.facts.bindings[child].parent = Some(compiler::SourceSchemaId::Record(
                    CtRecordSchemaId(f.facts.bindings[sibling].schema),
                ))
            }
            4 => {
                f.facts.bindings[child].site = f
                    .facts
                    .bindings
                    .iter()
                    .find(|binding| binding.parent.is_none())
                    .unwrap()
                    .site
            }
            5 => {
                f.facts.bindings.remove(child);
            }
            6 => {
                f.facts.bindings.push(f.facts.bindings[child].clone());
            }
            7 => {
                let module = &f.context.modules[0].module;
                let s::ItemKind::Aggregate(leaf) = &module.items[0].kind else {
                    unreachable!()
                };
                let site = compiler::source_sites::select(
                    module,
                    compiler::source_sites::Selection::Type(
                        &leaf.fields[0].ty as *const s::TypeRef as usize,
                    ),
                    Limits::default().max_call_depth,
                    |_| Ok(()),
                )
                .unwrap()
                .unwrap()
                .site;
                f.facts.bindings[child].site = Some(site);
            }
            9 => f.facts.bindings[child].caller = Some(f.raw.entry),
            10 => {
                f.facts.bindings[child].definition =
                    f.raw.functions[f.raw.entry.0 as usize].definition.clone()
            }
            _ => {
                // The sibling field is a real source Type. The entire parent
                // raw field layout must defeat coordinated graph+site+alias
                // changes, even when both lifetimes share one declaration.
                let schema = f.facts.bindings[child].schema as usize;
                let other = f.facts.bindings[sibling].schema as usize;
                let replacement = f.raw.record_schemas[other].life_context.clone().unwrap();
                f.raw.record_schemas[schema].life_context = Some(replacement);
                f.facts.bindings[child].site = f.facts.bindings[sibling].site;
                f.facts.bindings[child].parameters = f.facts.bindings[sibling].parameters.clone();
                f.facts.bindings[child].sources = f.facts.bindings[sibling].sources.clone();
            }
        }
        let limits = Limits::default();
        let transported = zil_ir::ct_wire::decode_program(
            &zil_ir::ct_wire::encode_program(&f.raw, &limits).unwrap(),
            &limits,
        )
        .unwrap();
        assert!(
            f.context
                .validate_type_contexts(&f.facts, &transported, &f.index, &mut f.budget)
                .is_err(),
            "nominal field substitution {attack}"
        );
    }
}

#[test]
fn actual_generic_host_preserves_context_when_member_has_no_own_parameters() {
    let mut f = fixture(
        "struct Phantom<life a,T:Copy,#N:i32>{n:i32} struct Maker<life h,X:Copy,#M:i32>{fn make()->Type{return Phantom::<'h,X,M>}} fn choose<life s>()->Type{return Maker::<'s,bool,42>.make()} fn query<life s>(p:&'s i32)->i32{let value:Type=#(choose::<'s>());return 0}",
        3,
    );
    if let Err(error) = f.context.extend_calls(
        &f.facts.calls,
        &f.facts.scopes,
        &f.raw,
        &f.index,
        &mut f.budget,
    ) {
        let admitted = f
            .context
            .active
            .iter()
            .map(|active| &active.raw)
            .collect::<Vec<_>>();
        panic!(
            "host source admission: {error:?}; raw={:?}; admitted={admitted:?}; calls={:?}",
            f.raw.ambient_life_scopes, f.facts.calls
        );
    }
    f.context
        .validate_type_contexts(&f.facts, &f.raw, &f.index, &mut f.budget)
        .unwrap();
}
