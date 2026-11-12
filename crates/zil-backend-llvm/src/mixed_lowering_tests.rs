//! Raw inputs complement the actual Source/native mixed-project differential.
use super::*;

fn mixed(value: i32) -> VerifiedModule {
    let (mut module, mut plan) = scoped_fixture::fixture();
    let mut leaf = returning(Constant::I32(value));
    leaf.id = FunctionId(module.functions.len() as u32);
    leaf.name = "independent_scalar".into();
    plan.functions.push(FunctionPlan {
        function: leaf.id,
        holds: vec![ValueHold {
            value: ValueId(0),
            roots: vec![],
        }],
        blocks: vec![BlockPlan {
            block: BlockId(0),
            initialized_in: vec![],
            initialized_out: vec![],
        }],
    });
    module.functions.push(leaf);
    plan.module_fingerprint = module.fingerprint();
    let target = module.target.clone();
    verify_module(module, plan, &target, &Limits::default()).unwrap()
}

#[test]
fn mixed_closure_scalar_edit_reuses_only_admitted_functions_and_matches_fresh_machine() {
    let options = EmitOptions {
        entry: EntryPoint::None,
    };
    for (scalar_policy, closed_collection) in [(false, false), (true, false), (true, true)] {
        let mut session = crate::emit::incremental::Session::default();
        for (index, value) in [42, 42, 43, 43].into_iter().enumerate() {
            let verified = mixed(value);
            let raw = lir(&verified);
            let cached = crate::emit::lower_view_with_session(
                &verified,
                &raw.as_view(),
                &options,
                scalar_policy,
                closed_collection,
                1,
                Some(&mut session),
            )
            .unwrap();
            let expected = crate::emit::lower_view_with_session(
                &verified,
                &raw.as_view(),
                &options,
                scalar_policy,
                closed_collection,
                1,
                None,
            )
            .unwrap();
            assert_eq!(
                cached, expected,
                "all current helpers, intrinsics and budget must match"
            );
            assert_eq!(cached.0.to_text(), expected.0.to_text());
            assert!(session.identity().is_none());
            assert!(!session.eligible);
            assert!(
                session.misses > 0,
                "closure entries remain actual original emissions"
            );
            assert_eq!(session.hits, usize::from(index == 1 || index == 3));
        }
    }
    let mut session = crate::emit::incremental::Session::default();
    let verified = mixed(43);
    let raw = lir(&verified);
    crate::emit::lower_view_with_session(
        &verified,
        &raw.as_view(),
        &options,
        false,
        false,
        1,
        Some(&mut session),
    )
    .unwrap();
    // Reusing a body never grants admission to substituted current raw LIR.
    let verified = mixed(43);
    let mut raw = lir(&verified);
    raw.closures[0].kind = CallableKind::FnOnce;
    let errors = crate::emit::lower_view_with_session(
        &verified,
        &raw.as_view(),
        &options,
        false,
        false,
        1,
        Some(&mut session),
    )
    .unwrap_err();
    assert_eq!(errors[0].code, "E-CODEGEN-UNBOUND-LIR");
}
