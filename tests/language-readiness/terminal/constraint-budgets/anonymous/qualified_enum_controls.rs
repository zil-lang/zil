//! Qualified pure Enum metadata across real Source, Core, Typed, plan and Wire.
use zil_base::{FileId, Limits, ModuleId, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{CompileOptions, Stage};
use zil_ir::{LifeNominalKind, LifeTypeProjection, Type};

const SOURCE: &str = "struct Phantom<life a>{n:i32}enum Bag<life a,T:Copy,#N:i32>{Empty,Some(T,[T;2])}fn result<life s>(p:&'s i32)->i32{let x=Bag::<'s,Phantom<'s>,3>.Some(Phantom::<'s>{n:20},[Phantom::<'s>{n:21},Phantom::<'s>{n:22}]);match x{Bag.Some(v,xs)=>{return v.n+xs[0].n+*p},Bag.Empty()=>{return 0}}}fn main()->i32{let n=1;return result(&n)}";

#[test]
fn qualified_enum_array_source_raw_wire_and_nearest_forgery() {
    let source = SourceUnit::new(FileId(0), ModuleId(0), "g4463-enum.zil", SOURCE);
    let options = CompileOptions {
        stop_after: Stage::Verify,
        ..CompileOptions::default()
    };
    let report = zil_driver::compile_source(&source, &options);
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let syntax = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand_with_map_limits_target_context(
        &source,
        &syntax,
        &limits,
        &target,
        zil_expand::source_context::SourceIdentityContext::ClosedSnapshot {
            root: source.module_id,
            aliases: &Default::default(),
            declarations: zil_expand::source_context::DeclarationPlacement::Original,
        },
    )
    .unwrap()
    .core;
    let enum_index = core
        .life_instances
        .iter()
        .position(|i| {
            core.life_declarations
                .iter()
                .any(|d| d.id == i.declaration && d.kind == LifeNominalKind::Enum)
        })
        .expect("real Enum instance");
    let mut wrong_core = core.clone();
    let Type::BorrowedStorage { storage, .. } = &mut wrong_core.life_instances[enum_index].ty
    else {
        unreachable!()
    };
    let Type::Enum { variants, .. } = storage.as_mut() else {
        unreachable!()
    };
    variants[1].1[0] = Type::Bool;
    assert!(
        zil_types::check(&wrong_core, &target, &limits).is_err(),
        "Types accepted forged Enum payload"
    );

    let raw = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&raw, &limits).unwrap();
    zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap();
    let mut missing = raw.clone();
    missing.life_instances.remove(enum_index);
    let error = zil_analyze::analyze(&missing, &limits).unwrap_err();
    assert!(
        error.iter().any(|d| d.message.contains("stored")),
        "{error:?}"
    );
    let error = zil_verify::verify_module(missing, plan.clone(), &target, &limits).unwrap_err();
    assert!(
        error.iter().any(|d| d.message.contains("stored")),
        "{error:?}"
    );
    let mut no_directory = raw.clone();
    no_directory.life_declarations.clear();
    no_directory.life_instances.clear();
    for function in &mut no_directory.functions {
        function.storage_life = None;
    }
    let error = zil_analyze::analyze(&no_directory, &limits).unwrap_err();
    assert!(
        error.iter().any(|d| d.message.contains("stored")),
        "{error:?}"
    );
    let error =
        zil_verify::verify_module(no_directory, plan.clone(), &target, &limits).unwrap_err();
    assert!(
        error.iter().any(|d| d.message.contains("stored")),
        "{error:?}"
    );
    let bytes = zil_ir::wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap();
    let Type::BorrowedStorage { storage, life } = &raw.life_instances[enum_index].ty else {
        unreachable!()
    };
    let Type::Enum { variants, .. } = storage.as_ref() else {
        unreachable!()
    };
    assert!(
        matches!(&variants[1].1[1], Type::Array { element, .. } if matches!(element.as_ref(), Type::BorrowedStorage { .. }))
    );
    assert!(life.slots.is_empty());
    assert!(
        life.retained
            .iter()
            .any(|r| r.path.contains(&LifeTypeProjection::Element))
    );

    let mut wrong_raw = raw.clone();
    let Type::BorrowedStorage { life, .. } = &mut wrong_raw.life_instances[enum_index].ty else {
        unreachable!()
    };
    life.retained
        .iter_mut()
        .find(|r| r.path.contains(&LifeTypeProjection::Element))
        .expect("array retained class")
        .path
        .pop();
    let error = zil_analyze::analyze(&wrong_raw, &limits).unwrap_err();
    assert!(
        error.iter().any(|d| d.message.contains("stored")),
        "{error:?}"
    );
    let error =
        zil_verify::verify_module(wrong_raw.clone(), plan.clone(), &target, &limits).unwrap_err();
    assert!(
        error.iter().any(|d| d.message.contains("stored")),
        "{error:?}"
    );
    let wrong_bytes = zil_ir::wire::encode_module_candidate(&wrong_raw, &plan, &limits).unwrap();
    let error = zil_verify::verify_module_bytes(&wrong_bytes, &target, &limits).unwrap_err();
    assert!(
        error.iter().any(|d| d.message.contains("stored")),
        "{error:?}"
    );

    for bad_field in [
        Type::Address {
            mutable: false,
            inner: Box::new(Type::I32),
        },
        Type::Managed(Box::new(Type::I32)),
    ] {
        let mut hostile = raw.clone();
        let Type::BorrowedStorage { storage, .. } = &mut hostile.life_instances[enum_index].ty
        else {
            unreachable!()
        };
        let Type::Enum { variants, .. } = storage.as_mut() else {
            unreachable!()
        };
        variants[1].1[0] = bad_field;
        let error = zil_analyze::analyze(&hostile, &limits).unwrap_err();
        assert!(
            error.iter().any(|d| d.message.contains("stored")),
            "{error:?}"
        );
        let error = zil_verify::verify_module(hostile, plan.clone(), &target, &limits).unwrap_err();
        assert!(
            error.iter().any(|d| d.message.contains("stored")),
            "{error:?}"
        );
    }
    let mut hidden = raw.clone();
    let Type::BorrowedStorage { storage, .. } = &mut hidden.life_instances[enum_index].ty else {
        unreachable!()
    };
    let Type::Enum { variants, .. } = storage.as_mut() else {
        unreachable!()
    };
    variants[1].1[1] = Type::Array {
        element: Box::new(Type::Address {
            mutable: false,
            inner: Box::new(Type::I32),
        }),
        length: 2,
    };
    let error = zil_analyze::analyze(&hidden, &limits).unwrap_err();
    assert!(
        error.iter().any(|d| d.message.contains("stored")),
        "{error:?}"
    );
    let error = zil_verify::verify_module(hidden, plan, &target, &limits).unwrap_err();
    assert!(
        error.iter().any(|d| d.message.contains("stored")),
        "{error:?}"
    );
}

#[test]
fn array_signature_cannot_drop_its_complete_class_contract() {
    for addressed in [false, true] {
        let (parameter, result, body, argument, observe) = if addressed {
            ("&[Leaf<'a>;1]", "i32", "return value[0].n", "&values", "")
        } else {
            (
                "[Leaf<'a>;1]",
                "[Leaf<'a>;1]",
                "return value",
                "values",
                "[0].n",
            )
        };
        let text = format!(
            "struct Leaf<life a>{{n:i32}}fn keep<life a>(token:&'a i32,value:{parameter})->{result}{{{body}}}fn read<life s>(token:&'s i32)->i32{{let values=[Leaf::<'s>{{n:42}}];let returned=keep(token,{argument});return returned{observe}}}fn main()->i32{{let token=0;return read(&token)}}"
        );
        let source = SourceUnit::new(FileId(0), ModuleId(0), "g4463-contract.zil", text);
        if addressed {
            // Root addresses used by array indexing are implemented; admitting
            // new addressed-array function parameters is a separate boundary.
            let report = zil_driver::compile_source(&source, &CompileOptions::default());
            assert_eq!(
                report.status,
                ResultCategory::Unsupported,
                "{}",
                report.to_json()
            );
            assert_eq!(report.phase, zil_base::Phase::Types, "{}", report.to_json());
            assert!(
                report
                    .diagnostics
                    .iter()
                    .any(|d| d.message == "resource/address parameter mode"),
                "{}",
                report.to_json()
            );
            continue;
        }
        let limits = Limits::default();
        let target = TargetProfile::host_llvm();
        let syntax = zil_syntax::parse(&source).into_result().unwrap();
        let core = zil_expand::expand_with_map_limits_target_context(
            &source,
            &syntax,
            &limits,
            &target,
            zil_expand::source_context::SourceIdentityContext::ClosedSnapshot {
                root: source.module_id,
                aliases: &Default::default(),
                declarations: zil_expand::source_context::DeclarationPlacement::Original,
            },
        )
        .unwrap()
        .core;
        let raw = zil_types::check(&core, &target, &limits).unwrap();
        let plan = zil_analyze::analyze(&raw, &limits).unwrap();
        zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap();
        let positive_wire = zil_ir::wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
        zil_verify::verify_module_bytes(&positive_wire, &target, &limits).unwrap();
        let mut missing = raw.clone();
        let function = missing.functions.iter_mut().find(|function| {
            function.parameters.iter().any(|id| {
                let ty = &function.locals[id.0 as usize].ty;
                if addressed {
                    matches!(ty, Type::Address { inner, .. } if matches!(inner.as_ref(), Type::Array { .. }))
                } else {
                    matches!(ty, Type::Array { .. })
                }
            })
        }).expect("actual array parameter");
        let contract = function.storage_life.take().expect("actual class contract");
        assert!(
            contract
                .classes
                .iter()
                .any(|row| row.position.path.contains(&LifeTypeProjection::Element))
        );
        // Preserve the complete actual types and nominal directory. Only the
        // relation connecting the real nested classes to the signature is gone.
        assert_eq!(missing.life_instances, raw.life_instances);
        let analyzed = zil_analyze::analyze(&missing, &limits).unwrap_err();
        // A coherent hostile claim must reach the independent class checker,
        // rather than being rejected only for the old module fingerprint.
        let mut claim = plan.clone();
        claim.module_fingerprint = missing.fingerprint();
        let verified = zil_verify::verify_module(missing.clone(), claim.clone(), &target, &limits)
            .unwrap_err();
        let bytes = zil_ir::wire::encode_module_candidate(&missing, &claim, &limits).unwrap();
        let decoded = zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap_err();
        for errors in [analyzed, verified, decoded] {
            assert!(
                errors.iter().any(|d| d.message.contains(
                    "nested stored qualified signature requires complete class relation"
                )),
                "addressed={addressed}: {errors:?}"
            );
        }
    }
}
