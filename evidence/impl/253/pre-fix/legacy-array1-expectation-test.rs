#[test]
fn impl247_owned_signature_does_not_grant_state_array_or_opaque_profiles() {
    let base = owned_callback_fixture(CallableKind::FnOnce, 1);
    for ty in [
        Type::Package {
            name: "State".into(),
            fields: vec![
                ("token".into(), managed()),
                ("callback".into(), Type::Closure(ClosureId(0))),
            ],
        },
        Type::Array {
            element: Box::new(Type::Closure(ClosureId(0))),
            length: 1,
        },
    ] {
        let mut m = base.clone();
        let local = LocalId(m.functions[0].locals.len() as u32);
        m.functions[0].locals.push(LocalDecl {
            id: local,
            name: "unused_container".into(),
            ty,
            mutable: false,
            span: s(),
        });
        let errors = analyze(&m, &Limits::default()).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|d| d.message.contains("does not grant a State")),
            "{errors:?}"
        );
        let bytes = zil_ir::wire::encode_module_candidate(
            &m,
            &CandidatePlan::empty_for(&m),
            &Limits::default(),
        )
        .unwrap();
        let raw = zil_ir::wire::decode_module_candidate(&bytes, &Limits::default())
            .unwrap()
            .0;
        assert!(analyze(&raw, &Limits::default()).is_err());
    }
    let mut m = base;
    m.functions[0].callable_return_contract = Some(CallableReturnContract {
        source: zil_ir::CallableReturnSource::Fresh,
        closure: ClosureId(0),
        kind: CallableKind::FnOnce,
        hold: vec![],
        call: vec![],
        writes: vec![],
        cleanup: vec![],
        lending_captures: vec![],
    });
    let errors = analyze(&m, &Limits::default()).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|d| d.message.contains("does not grant an opaque factory")),
        "{errors:?}"
    );
}
