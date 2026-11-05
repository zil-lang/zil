//! Actual project cache consumers; no handwritten nominal inventory positive.
use zil_base::{Limits, ResultCategory};
use zil_driver::{CompileOptions, Stage};
use zil_ir::{LifeNominalActual, LifeNominalKind, LifeParameterKind, LifeTypeProjection, Type};

#[test]
fn g4461_actual_core_nested_type_metadata_controls() {
    let source = zil_base::SourceUnit::new(
        zil_base::FileId(0),
        zil_base::ModuleId(0),
        "g4461-core.zil",
        "struct Phantom<life a,life b,T:Copy,#N:i32>{n:i32}\nstruct Wrap<T:Copy>{n:i32}\nfn result<life s>(p:&'s i32)->i32{let value=Wrap::<Phantom<'s,'s,i32,42>>{n:42};return value.n}\nfn main()->i32{let n=0;return result(&n)}",
    );
    let limits = Limits::default();
    let target = zil_base::TargetProfile::host_llvm();
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
    zil_types::check(&core, &target, &limits).unwrap();
    let outer = core.life_instances.iter().position(|i| matches!(&i.ty, Type::BorrowedStorage{life,..} if life.nominal_arguments.iter().any(|r| !r.path.is_empty()))).unwrap();
    let child_type = match &core.life_instances[outer].arguments[0] {
        LifeNominalActual::Type(t) => t,
        _ => unreachable!(),
    };
    let child = core
        .life_instances
        .iter()
        .position(|i| i.ty == *child_type)
        .unwrap();
    for mutation in 0..6 {
        let mut hostile = core.clone();
        match mutation {
            0..=3 => {
                let Type::BorrowedStorage { life, .. } = &mut hostile.life_instances[outer].ty
                else {
                    unreachable!()
                };
                match mutation {
                    0 => life.nominal_arguments[0].path = vec![LifeTypeProjection::TypeArgument(2)],
                    1 => life.nominal_arguments[0].argument = 2,
                    2 => {
                        life.nominal_arguments.pop();
                    }
                    3 => {
                        life.binder_count = 2;
                        life.nominal_arguments[1].binder = 1;
                    }
                    _ => unreachable!(),
                }
            }
            4 => {
                hostile.life_instances.remove(child);
            }
            5 => {
                hostile.life_instances[outer].arguments[0] = LifeNominalActual::Type(Type::I32);
            }
            _ => unreachable!(),
        }
        assert!(
            zil_types::check(&hostile, &target, &limits).is_err(),
            "Types accepted mutation {mutation}"
        );
    }
}

#[test]
fn g4461_actual_nested_type_nominal_source_raw_wire() {
    let text = include_str!(
        "../../tests/language-readiness/terminal/344/anonymous/ct6-ambient-type-actual.zil"
    )
    .replace("life a,T:Copy", "life a,life b,T:Copy")
    .replace("Phantom<'s,i32,42>", "Phantom<'s,'s,i32,42>");
    let directory = super::Temp::new();
    let cache = directory.0.join("cache");
    let report = zil_driver::compile_project(
        &super::review329_project(&[("main", &text)]),
        &CompileOptions {
            stop_after: Stage::Verify,
            semantic_cache: Some(cache.clone()),
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    let limits = Limits::default();
    let store = cache.join("semantic-v4");
    let mut count = 0;
    for entry in std::fs::read_dir(&store).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|e| e != "candidate") {
            continue;
        }
        let key =
            zil_base::Fingerprint::from_hex(path.file_stem().unwrap().to_str().unwrap()).unwrap();
        let zil_db::CacheRead::Candidate(payload) =
            zil_db::ArtifactCache::with_limit(&store, limits.max_source_bytes)
                .inspect_candidate(key)
                .unwrap()
        else {
            panic!("missing actual cache candidate")
        };
        let length = u32::from_le_bytes(payload[8..12].try_into().unwrap()) as usize;
        let (raw, plan) =
            zil_ir::wire::decode_module_candidate(&payload[12..12 + length], &limits).unwrap();
        zil_analyze::analyze(&raw, &limits).unwrap();
        zil_verify::verify_module(raw.clone(), plan.clone(), &raw.target, &limits).unwrap();
        let bytes = zil_ir::wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
        zil_verify::verify_module_bytes(&bytes, &raw.target, &limits).unwrap();
        let outer = raw.life_instances.iter().position(|i| matches!(&i.ty, Type::BorrowedStorage{life,..} if life.nominal_arguments.iter().any(|r| !r.path.is_empty()))).expect("real outer TypeArgument inventory");
        let Type::BorrowedStorage { life, .. } = &raw.life_instances[outer].ty else {
            unreachable!()
        };
        assert_eq!(life.nominal_arguments.len(), 2);
        assert!(
            life.nominal_arguments
                .iter()
                .all(|r| r.path == [LifeTypeProjection::TypeArgument(0)] && r.binder == 0)
        );
        let child_type = match &raw.life_instances[outer].arguments[0] {
            LifeNominalActual::Type(t) => t,
            _ => unreachable!(),
        };
        let child = raw
            .life_instances
            .iter()
            .position(|i| i.ty == *child_type)
            .unwrap();
        let child_decl = raw
            .life_declarations
            .iter()
            .position(|d| d.id == raw.life_instances[child].declaration)
            .unwrap();
        for mutation in 0..12 {
            let mut hostile = raw.clone();
            match mutation {
                0..=5 => {
                    let Type::BorrowedStorage { life, .. } = &mut hostile.life_instances[outer].ty
                    else {
                        unreachable!()
                    };
                    match mutation {
                        0 => {
                            life.nominal_arguments[0].path =
                                vec![LifeTypeProjection::TypeArgument(1)]
                        }
                        1 => life.nominal_arguments[0].declaration.declaration.0 += 100000,
                        2 => life.nominal_arguments[0].argument = 2,
                        3 => {
                            life.nominal_arguments.pop();
                        }
                        4 => life
                            .nominal_arguments
                            .push(life.nominal_arguments[0].clone()),
                        5 => {
                            life.binder_count = 2;
                            life.nominal_arguments[1].binder = 1;
                        }
                        _ => unreachable!(),
                    }
                }
                6 => {
                    hostile.life_instances.remove(child);
                }
                7 => hostile
                    .life_instances
                    .push(hostile.life_instances[child].clone()),
                8 => hostile.life_declarations[child_decl].kind = LifeNominalKind::Enum,
                9 => {
                    hostile.life_declarations[child_decl].parameters.pop();
                }
                10 => hostile.life_declarations[child_decl].parameters[0] = LifeParameterKind::Type,
                11 => {
                    hostile.life_instances[outer].arguments[0] = LifeNominalActual::Type(Type::I32);
                }
                _ => unreachable!(),
            }
            assert!(
                zil_analyze::analyze(&hostile, &limits).is_err(),
                "Analyze accepted mutation {mutation}"
            );
            let mut claim = plan.clone();
            claim.module_fingerprint = hostile.fingerprint();
            let bytes = zil_ir::wire::encode_module_candidate(&hostile, &claim, &limits).unwrap();
            assert!(
                zil_verify::verify_module(hostile, claim, &raw.target, &limits).is_err(),
                "Verify accepted mutation {mutation}"
            );
            assert!(
                zil_verify::verify_module_bytes(&bytes, &raw.target, &limits).is_err(),
                "Wire accepted mutation {mutation}"
            );
        }
        count += 1;
    }
    assert_eq!(count, 1);
}
