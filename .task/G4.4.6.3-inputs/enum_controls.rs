//! Qualified pure Enum metadata across real Source, Core, Typed, plan and Wire.
use zil_base::{FileId, Limits, ModuleId, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{CompileOptions, Stage};
use zil_ir::{LifeNominalKind, LifeTypeProjection, Type};

const SOURCE: &str = "struct Phantom<life a>{n:i32}enum Bag<life a,T:Copy,#N:i32>{Empty,Some(T,[T;2])}fn result<life s>(p:&'s i32)->i32{let x=Bag::<'s,Phantom<'s>,3>.Some(Phantom::<'s>{n:20},[Phantom::<'s>{n:21},Phantom::<'s>{n:22}]);match x{Bag.Some(v,xs)=>{return v.n+xs[0].n+*p},Bag.Empty()=>{return 0}}}fn main()->i32{let n=1;return result(&n)}";

#[test]
fn g4463_qualified_enum_array_source_raw_wire_and_nearest_forgery() {
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
