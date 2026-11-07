//! Actual project cache consumers; no handwritten nominal inventory positive.
use zil_base::{Limits, ResultCategory};
use zil_driver::{CompileOptions, Stage};
use zil_ir::{LifeNominalActual, LifeNominalKind, LifeParameterKind, LifeTypeProjection, Type};

#[test]
fn core_named_fields_and_actuals_get_resolved_physical_checks() {
    for actual in ["i32", "Pair"] {
        let text = format!(
            "struct Pair{{left:i32;right:u64}}enum Choice{{Empty,Pair(i32,u64)}}struct Phantom<life a,T:Copy,#N:i32>{{tuple:(i32,bool);array:[i32;2];record:Pair;variant:Choice}}fn result<life s>(p:&'s i32)->u64{{let value=Phantom::<'s,{actual},37>{{tuple:(37,true),array:[37,38],record:Pair{{left:39,right:18446744073709551557u64}},variant:Choice.Pair(40,18446744073709551557u64)}};return value.record.right}}fn main()->u64{{let n=0;return result(&n)}}"
        );
        let source = zil_base::SourceUnit::new(
            zil_base::FileId(0),
            zil_base::ModuleId(0),
            "nominal-lifetime-core-named.zil",
            text,
        );
        let options = CompileOptions {
            stop_after: Stage::Types,
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
        let instance = &core.life_instances[0];
        let Type::BorrowedStorage { storage, .. } = &instance.ty else {
            unreachable!()
        };
        let Type::Record { name, fields } = storage.as_ref() else {
            unreachable!()
        };
        assert!(matches!(&fields[2].1, Type::Named(_)));
        assert!(matches!(&fields[3].1, Type::Named(_)));
        let aggregate = core
            .aggregates
            .iter()
            .position(|a| {
                a.name == *name || format!("@module{}:{}", core.module_id.0, a.name) == *name
            })
            .unwrap();
        if actual == "Pair" {
            assert!(matches!(
                &instance.arguments[1],
                LifeNominalActual::Type(Type::Named(_))
            ));
        }
        let typed = zil_types::check(&core, &target, &limits).unwrap();
        let Type::BorrowedStorage { storage, .. } = &typed.life_instances[0].ty else {
            unreachable!()
        };
        let Type::Record { fields, .. } = storage.as_ref() else {
            unreachable!()
        };
        assert!(matches!(&fields[2].1,Type::Record{fields,..} if fields[1].1==Type::U64));
        assert!(matches!(&fields[3].1, Type::Enum { .. }));
        if actual == "Pair" {
            assert!(matches!(
                &typed.life_instances[0].arguments[1],
                LifeNominalActual::Type(Type::Record { .. })
            ));
        }
        for mutation in 0..5 {
            let mut hostile = core.clone();
            match mutation {
                0 => hostile.life_instances[0].arguments[0] = LifeNominalActual::Type(Type::I32),
                1 => {
                    hostile.life_instances[0].arguments[1] =
                        LifeNominalActual::Type(Type::Named("$absent_actual".into()))
                }
                2 => {
                    let Type::BorrowedStorage { storage, .. } = &mut hostile.life_instances[0].ty
                    else {
                        unreachable!()
                    };
                    let Type::Record { fields, .. } = storage.as_mut() else {
                        unreachable!()
                    };
                    fields[2].1 = Type::Bool;
                }
                3 => {
                    let Type::BorrowedStorage { life, .. } = &mut hostile.life_instances[0].ty
                    else {
                        unreachable!()
                    };
                    life.slots.push(zil_ir::StorageLifeSlot {
                        path: vec![zil_ir::PlaceProjection::Field(2)],
                        binder: 0,
                        mode: zil_ir::StoredBorrowMode::SharedAddress,
                    });
                }
                4 => {
                    let invalid = Type::Named("$absent_physical".into());
                    hostile.aggregates[aggregate].fields[2].ty = invalid.clone();
                    let Type::BorrowedStorage { storage, .. } = &mut hostile.life_instances[0].ty
                    else {
                        unreachable!()
                    };
                    let Type::Record { fields, .. } = storage.as_mut() else {
                        unreachable!()
                    };
                    fields[2].1 = invalid;
                }
                _ => unreachable!(),
            }
            assert!(
                zil_types::check(&hostile, &target, &limits).is_err(),
                "Types accepted unresolved/physical mutation {mutation}"
            );
        }
    }
}

#[test]
fn same_child_distinct_outer_relations_coexist() {
    let text = "struct Phantom<life a>{n:i32}struct Wrap<T:Copy,U:Copy>{n:i32}fn same<life s>(p:&'s i32)->i32{let w=Wrap::<Phantom<'s>,Phantom<'s>>{n:20};return w.n}fn apart<life a,life b>(p:&'a i32,q:&'b i32)->i32{let w=Wrap::<Phantom<'a>,Phantom<'b>>{n:22};return w.n}fn main()->i32{let p=0;let q=1;return same(&p)+apart(&p,&q)}";
    let source = zil_base::SourceUnit::new(
        zil_base::FileId(0),
        zil_base::ModuleId(0),
        "nominal-lifetime-relations.zil",
        text,
    );
    let report = zil_driver::compile_source(
        &source,
        &CompileOptions {
            stop_after: Stage::Verify,
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
}

#[test]
fn actual_pure_fields_retained_source_raw_wire() {
    for text in [
        "struct Phantom<life a,T:Copy,#N:i32>{n:i32}struct View<life a>{p:&'a i32}struct Wrap<T:Copy>{value:T}fn result<life s>(p:&'s i32)->i32{let borrowed=View::<'s>{p:p};let w=Wrap::<Phantom<'s,i32,42>>{value:Phantom::<'s,i32,42>{n:42}};let copied=w.value;return copied.n+*borrowed.p}fn main()->i32{let n=0;return result(&n)}",
        "struct Phantom<life a>{n:i32}struct View<life a>{p:&'a i32}struct Wrap<T:Copy,U:Copy>{left:T;right:(U)}fn result<life a,life b>(p:&'a i32,q:&'b i32)->i32{let borrowed=View::<'a>{p:p};let w=Wrap::<Phantom<'a>,Phantom<'b>>{left:Phantom::<'a>{n:20},right:Phantom::<'b>{n:22}};let left=w.left;let right=w.right;return left.n+right.n+*borrowed.p}fn main()->i32{let p=0;let q=1;return result(&p,&q)}",
    ] {
        let source = zil_base::SourceUnit::new(
            zil_base::FileId(0),
            zil_base::ModuleId(0),
            "nominal-lifetime-fields.zil",
            text,
        );
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
        let raw = zil_types::check(&core, &target, &limits).unwrap();
        let plan = zil_analyze::analyze(&raw, &limits).unwrap();
        zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap();
        let bytes = zil_ir::wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
        zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap();
        for (f, proof) in raw.functions.iter().zip(&plan.functions) {
            for i in f.blocks.iter().flat_map(|b| &b.instructions) {
                let (zil_ir::InstructionKind::ReadAddress { address }, Some(value)) =
                    (&i.kind, i.result)
                else {
                    continue;
                };
                let Type::BorrowedStorage { life, .. } = &f.values[value.0 as usize] else {
                    continue;
                };
                assert!(life.slots.is_empty());
                assert!(
                    matches!(&f.values[address.0 as usize], Type::Address {inner,..} if inner.as_ref()==&f.values[value.0 as usize])
                );
                assert!(
                    proof.holds[value.0 as usize].roots.is_empty(),
                    "phantom result must not acquire its storage root"
                );
            }
        }
        assert!(
            raw.functions
                .iter()
                .any(|f| f
                    .blocks
                    .iter()
                    .flat_map(|b| &b.instructions)
                    .any(
                        |i| matches!(i.kind, zil_ir::InstructionKind::ReadAddress { .. })
                            && i.result.is_some_and(|v| f
                                .values
                                .get(v.0 as usize)
                                .is_some_and(|ty| matches!(ty, Type::BorrowedStorage { .. })))
                    )),
            "actual Source must exercise whole qualified child reads"
        );
        // Insert an unsafe ordinary read of a real View with a physical
        // Address slot. Its input and result types agree and all inventory is
        // untouched; the narrow read proof itself must reject this copy.
        let location = raw.functions.iter().enumerate().find_map(|(fi, f)| {
            f.blocks.iter().enumerate().find_map(|(bi, b)| {
                b.instructions.iter().enumerate().find_map(|(ii, i)| {
                    let zil_ir::InstructionKind::LoadStoredBorrow { storage, .. } = &i.kind else {
                        return None;
                    };
                    Some((fi, bi, ii, *storage, i.span))
                })
            })
        }).expect("each real field source must contain the borrowed View slot load used by the hostile ordinary-read control");
        {
            let (fi, bi, ii, address, span) = location;
            let mut hostile = raw.clone();
            let f = &mut hostile.functions[fi];
            let Type::Address { inner, .. } = &f.values[address.0 as usize] else {
                unreachable!()
            };
            let payload = inner.as_ref().clone();
            assert!(matches!(&payload,Type::BorrowedStorage{life,..} if !life.slots.is_empty()));
            let value = zil_ir::ValueId(f.values.len() as u32);
            f.values.push(payload);
            f.blocks[bi].instructions.insert(
                ii,
                zil_ir::Instruction {
                    result: Some(value),
                    kind: zil_ir::InstructionKind::ReadAddress { address },
                    span,
                },
            );
            let errors = zil_analyze::analyze(&hostile, &limits)
                .expect_err("ordinary read copied an actual borrowed View");
            assert!(
                errors
                    .iter()
                    .any(|e| e.message.contains("ordinary read cannot copy")),
                "{errors:?}"
            );
            let mut claim = plan.clone();
            claim.module_fingerprint = hostile.fingerprint();
            let bytes = zil_ir::wire::encode_module_candidate(&hostile, &claim, &limits).unwrap();
            let error = zil_verify::verify_module(hostile, claim, &target, &limits)
                .expect_err("Verify copied an actual borrowed View");
            assert!(
                error.iter().any(|e| e.message.contains("ReadAddress")),
                "{error:?}"
            );
            assert!(zil_verify::verify_module_bytes(&bytes, &target, &limits).is_err());
        }
        let outer = raw
            .life_instances
            .iter()
            .position(
                |i| matches!(&i.ty, Type::BorrowedStorage{life,..} if !life.retained.is_empty()),
            )
            .unwrap();
        for mutation in 0..6 {
            let mut hostile = raw.clone();
            let Type::BorrowedStorage { life, .. } = &mut hostile.life_instances[outer].ty else {
                unreachable!()
            };
            match mutation {
                0 => {
                    life.retained.pop();
                }
                1 => life.retained[0].path = vec![LifeTypeProjection::TypeArgument(0)],
                2 => life.retained[0].class += 1,
                3 => life.retained[0].binder = life.binder_count,
                4 => life.retained.push(life.retained[0].clone()),
                5 => {
                    let Type::BorrowedStorage { storage, .. } =
                        &mut hostile.life_instances[outer].ty
                    else {
                        unreachable!()
                    };
                    let Type::Record { fields, .. } = storage.as_mut() else {
                        unreachable!()
                    };
                    let Type::BorrowedStorage { storage, .. } = &mut fields[0].1 else {
                        unreachable!()
                    };
                    let Type::Record { fields, .. } = storage.as_mut() else {
                        unreachable!()
                    };
                    fields[0].1 = Type::Address {
                        mutable: false,
                        inner: Box::new(Type::I32),
                    };
                }
                _ => unreachable!(),
            }
            assert!(
                zil_analyze::analyze(&hostile, &limits).is_err(),
                "Analyze accepted retained mutation {mutation}"
            );
            let mut claim = plan.clone();
            claim.module_fingerprint = hostile.fingerprint();
            let bytes = zil_ir::wire::encode_module_candidate(&hostile, &claim, &limits).unwrap();
            assert!(
                zil_verify::verify_module(hostile, claim, &target, &limits).is_err(),
                "Verify accepted retained mutation {mutation}"
            );
            assert!(
                zil_verify::verify_module_bytes(&bytes, &target, &limits).is_err(),
                "Wire accepted retained mutation {mutation}"
            );
        }
    }
}

#[test]
fn actual_core_nested_type_metadata_controls() {
    let source = zil_base::SourceUnit::new(
        zil_base::FileId(0),
        zil_base::ModuleId(0),
        "nominal-lifetime-core.zil",
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
    for mutation in 0..8 {
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
            6 => {
                let mut alternate = hostile.life_instances[outer].clone();
                let Type::BorrowedStorage { storage, .. } = &mut alternate.ty else {
                    unreachable!()
                };
                let Type::Record { name, .. } = storage.as_mut() else {
                    unreachable!()
                };
                let mut aggregate = hostile
                    .aggregates
                    .iter()
                    .find(|a| {
                        *name == a.name
                            || *name == format!("@module{}:{}", hostile.module_id.0, a.name)
                    })
                    .unwrap()
                    .clone();
                aggregate.name.push_str("_alternate_physical_identity");
                *name = format!("@module{}:{}", hostile.module_id.0, aggregate.name);
                hostile.aggregates.push(aggregate);
                hostile.life_instances.push(alternate);
            }
            7 => {
                let mut alternate = hostile.life_instances[child].clone();
                let actual = alternate
                    .arguments
                    .iter_mut()
                    .find(|a| matches!(a, LifeNominalActual::Const(_)))
                    .unwrap();
                *actual = LifeNominalActual::Const(zil_ir::Constant::I32(43));
                hostile.life_instances.push(alternate);
            }
            _ => unreachable!(),
        }
        let errors = zil_types::check(&hostile, &target, &limits)
            .expect_err("Types accepted hostile actual metadata");
        if mutation == 6 {
            assert!(
                errors
                    .iter()
                    .any(|e| e.message.contains("one complete nominal actual vector")),
                "{errors:?}"
            );
        }
    }
}

#[test]
fn actual_nested_type_nominal_source_raw_wire() {
    let text = include_str!(
        "../../../tests/language-readiness/terminal/constraint-budgets/anonymous/ct6-ambient-type-actual.zil"
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
        for mutation in 0..16 {
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
                12 => {
                    let mut alternate = hostile.life_instances[outer].clone();
                    let Type::BorrowedStorage { storage, .. } = &mut alternate.ty else {
                        unreachable!()
                    };
                    let Type::Record { name, .. } = storage.as_mut() else {
                        unreachable!()
                    };
                    name.push_str("_alternate_physical_identity");
                    hostile.life_instances.push(alternate);
                }
                13 => {
                    let mut alternate = hostile.life_instances[child].clone();
                    let actual = alternate
                        .arguments
                        .iter_mut()
                        .find(|a| matches!(a, LifeNominalActual::Const(_)))
                        .unwrap();
                    *actual = LifeNominalActual::Const(zil_ir::Constant::I32(43));
                    hostile.life_instances.push(alternate);
                }
                14 => {
                    hostile.life_instances[outer].arguments[0] =
                        LifeNominalActual::Type(hostile.life_instances[outer].ty.clone());
                }
                15 => {
                    hostile.life_declarations.remove(child_decl);
                }
                _ => unreachable!(),
            }
            let errors = zil_analyze::analyze(&hostile, &limits)
                .expect_err("Analyze accepted hostile actual metadata");
            if mutation == 12 {
                assert!(
                    errors
                        .iter()
                        .any(|e| e.message.contains("duplicate complete nominal instance")),
                    "{errors:?}"
                );
            }
            if mutation == 14 {
                assert!(
                    errors
                        .iter()
                        .any(|e| e.message.contains("cyclic nominal Type argument")),
                    "{errors:?}"
                );
            }
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
