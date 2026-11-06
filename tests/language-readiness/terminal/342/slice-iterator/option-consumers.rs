//! All mutations begin with genuine source-produced Core/Typed metadata.
use super::*;
use zil_ir::{LifeNominalActual, LifeTypeProjection, PlaceProjection, Type};

const HELPER: &str = include_str!("option-helper.zil");
type Mutation = (&'static str, fn(&mut TypedModule));

fn shape(module: &mut TypedModule) -> &mut zil_ir::StorageLifeShape {
    let Type::BorrowedStorage { life, .. } = &mut module.life_instances[0].ty else {
        panic!("source omitted qualified Option");
    };
    life
}

fn accept(module: &TypedModule) -> zil_ir::CandidatePlan {
    let limits = Limits::default();
    let plan = zil_analyze::analyze(module, &limits).unwrap();
    zil_verify::verify_module(module.clone(), plan.clone(), &module.target, &limits).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(module, &plan, &limits).unwrap();
    let (decoded, decoded_plan) = zil_ir::wire::decode_module_candidate(&wire, &limits).unwrap();
    assert_eq!(&decoded, module);
    assert_eq!(&decoded_plan, &plan);
    assert_eq!(zil_analyze::analyze(&decoded, &limits).unwrap(), plan);
    zil_verify::verify_module_bytes(&wire, &module.target, &limits).unwrap();
    plan
}

#[test]
fn g3_4_1_option_source_permissions_survive_raw_and_wire_consumers() {
    let module = typed(HELPER);
    assert_eq!(module.life_declarations.len(), 1);
    assert_eq!(module.life_instances.len(), 1);
    let Type::BorrowedStorage { life, .. } = &module.life_instances[0].ty else {
        panic!("source erased Option relation");
    };
    assert_eq!(life.retained[0].path, [LifeTypeProjection::TypeArgument(0)]);
    assert_eq!(
        life.slots[0].path,
        [PlaceProjection::Variant(zil_ir::VariantProjection {
            variant: 1,
            index: 0
        })]
    );
    let plan = accept(&module);
    assert!(
        plan.functions
            .iter()
            .flat_map(|f| &f.holds)
            .any(|h| h.roots.contains(&HoldRoot::Parameter(0))),
        "no actual input source survived Option construction"
    );
}

#[test]
fn g3_4_1_option_metadata_nearest_forgery_fails_each_raw_and_wire_kernel() {
    let module = typed(HELPER);
    let plan = accept(&module);
    let mutations: &[Mutation] = &[
        ("compiler flag alone", |m| {
            m.life_declarations[0].identity.package = "spoof".into()
        }),
        ("canonical builtin module", |m| {
            m.life_declarations[0].id.module.0 = 0
        }),
        ("canonical builtin ordinal", |m| {
            m.life_declarations[0].id.declaration.0 = 1
        }),
        ("canonical builtin module path", |m| {
            m.life_declarations[0].identity.module_path = vec!["other".into()]
        }),
        ("canonical builtin declaration path", |m| {
            m.life_declarations[0].identity.declaration_path[1] = "Other".into()
        }),
        ("canonical builtin generation", |m| {
            m.life_declarations[0].identity.generation_path.push(0)
        }),
        ("no builtin flag", |m| {
            m.life_declarations[0].id.compiler = false
        }),
        ("wrong full formal kind", |m| {
            m.life_declarations[0].parameters[0] = zil_ir::LifeParameterKind::Lifetime
        }),
        ("missing retained map", |m| shape(m).retained.clear()),
        ("wrong actual ordinal", |m| {
            shape(m).retained[0].path = vec![LifeTypeProjection::TypeArgument(1)]
        }),
        ("nominal row cannot stand in for address", |m| {
            let id = m.life_declarations[0].id;
            shape(m)
                .nominal_arguments
                .push(zil_ir::NominalLifeArgument {
                    path: vec![LifeTypeProjection::TypeArgument(0)],
                    declaration: id,
                    argument: 0,
                    binder: 0,
                });
        }),
        ("wrong local address class", |m| {
            shape(m).retained[0].class = 1
        }),
        ("conflicting enclosing binder", |m| {
            shape(m).retained[0].binder = 1
        }),
        ("duplicate class mapping", |m| {
            let row = shape(m).retained[0].clone();
            shape(m).retained.push(row);
        }),
        ("missing physical slot", |m| shape(m).slots.clear()),
        ("wrong physical variant", |m| {
            shape(m).slots[0].path = vec![PlaceProjection::Variant(zil_ir::VariantProjection {
                variant: 0,
                index: 0,
            })]
        }),
        ("wrong physical binder", |m| shape(m).slots[0].binder = 1),
        ("unused class", |m| shape(m).binder_count = 2),
        ("actual does not match payload", |m| {
            m.life_instances[0].arguments[0] = LifeNominalActual::Type(Type::I64)
        }),
        ("mutable actual", |m| {
            let LifeNominalActual::Type(Type::Address { mutable, .. }) =
                &mut m.life_instances[0].arguments[0]
            else {
                panic!()
            };
            *mutable = true;
        }),
        ("wrong real payload", |m| {
            let Type::BorrowedStorage { storage, .. } = &mut m.life_instances[0].ty else {
                panic!()
            };
            let Type::Enum { variants, .. } = storage.as_mut() else {
                panic!()
            };
            variants[1].1[0] = Type::I64;
        }),
        ("swapped real variants", |m| {
            let Type::BorrowedStorage { storage, .. } = &mut m.life_instances[0].ty else {
                panic!()
            };
            let Type::Enum { variants, .. } = storage.as_mut() else {
                panic!()
            };
            variants.swap(0, 1);
        }),
    ];
    for (label, mutate) in mutations {
        let mut forged = module.clone();
        mutate(&mut forged);
        assert!(
            zil_analyze::analyze(&forged, &Limits::default()).is_err(),
            "Analyze accepted {label}"
        );
        let mut claim = plan.clone();
        claim.module_fingerprint = forged.fingerprint();
        let wire =
            zil_ir::wire::encode_module_candidate(&forged, &claim, &Limits::default()).unwrap();
        let (decoded, _) =
            zil_ir::wire::decode_module_candidate(&wire, &Limits::default()).unwrap();
        assert!(
            zil_analyze::analyze(&decoded, &Limits::default()).is_err(),
            "Wire Analyze accepted {label}"
        );
        assert!(
            zil_verify::verify_module(forged, claim, &module.target, &Limits::default()).is_err(),
            "Verify accepted {label}"
        );
        assert!(
            zil_verify::verify_module_bytes(&wire, &module.target, &Limits::default()).is_err(),
            "Wire Verify accepted {label}"
        );
    }
}

#[test]
fn g3_4_1_option_wrong_actual_source_is_rejected_after_types() {
    let bad_source = HELPER.replace(
        "return Option::<&'s i64>.Some(&slice[0]);",
        "let local:i64=1; return Option::<&'s i64>.Some(&local);",
    );
    assert_ne!(bad_source, HELPER);
    let unit = source(&bad_source);
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    let errors = zil_expand::expand(&unit, &syntax).unwrap_err();
    assert!(
        errors.iter().any(|d| d.phase == Phase::Expand
            && d.category == zil_base::ResultCategory::SafeReject
            && d.code == "E-GENERIC-ARGUMENT"
            && d.message == "retained reference has no actual declared source class"),
        "{errors:?}"
    );

    // Source rejects the wrong life earlier. Challenge each downstream API
    // separately by changing the actual borrow target in genuine valid Core.
    let good_source = HELPER.replace(
        "return Option::<&'s i64>.Some(&slice[0]);",
        "let local:[i64;1]=[1];let local_slice=&local[..]; return Option::<&'s i64>.Some(&slice[0]);",
    );
    let unit = source(&good_source);
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &syntax).unwrap();
    let core = zil_types::infer_source_closures(&core, &Limits::default()).unwrap();
    let mut core =
        zil_ct::evaluate_source_constants(&core, &TargetProfile::default(), &Limits::default())
            .unwrap();
    let positive = zil_types::check(&core, &TargetProfile::default(), &Limits::default()).unwrap();
    let mut claim = accept(&positive);
    let mut changed = false;
    for f in &mut core.functions {
        for statement in &mut f.body.statements {
            if let zil_ir::CoreStatementKind::Return(Some(value)) = &mut statement.kind
                && let zil_ir::CoreExprKind::Call { arguments, .. } = &mut value.kind
                && let [argument] = arguments.as_mut_slice()
                && let zil_ir::CoreExprKind::Unary {
                    op: zil_ir::UnaryOp::Borrow,
                    operand,
                } = &mut argument.kind
            {
                let zil_ir::CoreExprKind::Index { object, .. } = &mut operand.kind else {
                    panic!("source omitted the actual slice index");
                };
                assert!(
                    matches!(&object.kind, zil_ir::CoreExprKind::Name(name) if name == "slice")
                );
                object.kind = zil_ir::CoreExprKind::Name("local_slice".into());
                changed = true;
            }
        }
    }
    assert!(changed, "source Core omitted the actual Some borrow");
    let module = zil_types::check(&core, &TargetProfile::default(), &Limits::default()).unwrap();
    // The candidate cannot fail merely because this raw body has another shape.
    for (actual, good) in module.functions.iter().zip(&positive.functions) {
        assert_eq!(actual.values, good.values);
        assert_eq!(actual.locals, good.locals);
        assert_eq!(actual.blocks.len(), good.blocks.len());
        for (a, g) in actual.blocks.iter().zip(&good.blocks) {
            assert_eq!(a.instructions.len(), g.instructions.len());
        }
    }
    let errors = zil_analyze::analyze(&module, &Limits::default()).unwrap_err();
    let assert_actual_source = |errors: &[zil_base::Diagnostic]| {
        assert!(errors.iter().any(|d| d.phase == Phase::Analyze
            && d.code == "E-ANALYZE-IR"
            && d.message == "stored target proof: stored return would retain an activation-local target"), "{errors:?}");
    };
    assert_actual_source(&errors);
    // Reuse only the valid source candidate, with its raw fingerprint refreshed.
    // Verify must reject from its own actual-body proof.
    claim.module_fingerprint = module.fingerprint();
    let wire = zil_ir::wire::encode_module_candidate(&module, &claim, &Limits::default()).unwrap();
    let (decoded, _) = zil_ir::wire::decode_module_candidate(&wire, &Limits::default()).unwrap();
    let errors = zil_analyze::analyze(&decoded, &Limits::default()).unwrap_err();
    assert_actual_source(&errors);
    // Verify independently checks execution windows before its final return
    // summary: the escaping Some loan conflicts with cleanup of its local array.
    let assert_actual_window = |errors: &[zil_base::Diagnostic]| {
        assert!(
            errors.iter().any(|d| d.phase == Phase::Verify
                && d.category == ResultCategory::SafeReject
                && d.code == "E-VERIFY-STORED"
                && d.message
                    == "stored target proof: actual target intersects a live incompatible loan"),
            "{errors:?}"
        );
    };
    let errors =
        zil_verify::verify_module(module.clone(), claim, &module.target, &Limits::default())
            .expect_err("raw Verify accepted the activation-local Some payload");
    assert_actual_window(&errors);
    let errors = zil_verify::verify_module_bytes(&wire, &module.target, &Limits::default())
        .expect_err("Wire Verify accepted the activation-local Some payload");
    assert_actual_window(&errors);
}

#[test]
fn g3_4_1_option_coherent_pointee_forgery_never_expands_the_shared_profile() {
    let module = typed(HELPER);
    let plan = accept(&module);
    for pointee in [
        Type::Managed(Box::new(Type::I64)),
        Type::CWideString,
        Type::Address {
            mutable: false,
            inner: Box::new(Type::I64),
        },
        Type::FunctionPointer {
            parameters: vec![],
            result: Box::new(Type::I64),
        },
        Type::Tuple(vec![Type::Address {
            mutable: false,
            inner: Box::new(Type::I64),
        }]),
        Type::Slice {
            element: Box::new(Type::Managed(Box::new(Type::I64))),
        },
    ] {
        let mut forged = module.clone();
        let address = Type::Address {
            mutable: false,
            inner: Box::new(pointee.clone()),
        };
        forged.life_instances[0].arguments[0] = LifeNominalActual::Type(address.clone());
        let Type::BorrowedStorage { storage, .. } = &mut forged.life_instances[0].ty else {
            panic!()
        };
        let Type::Enum { variants, .. } = storage.as_mut() else {
            panic!()
        };
        variants[1].1[0] = address;
        assert!(
            zil_analyze::analyze(&forged, &Limits::default()).is_err(),
            "Analyze accepted pointee {pointee:?}"
        );
        let mut claim = plan.clone();
        claim.module_fingerprint = forged.fingerprint();
        let wire =
            zil_ir::wire::encode_module_candidate(&forged, &claim, &Limits::default()).unwrap();
        assert!(
            zil_verify::verify_module(forged, claim, &module.target, &Limits::default()).is_err(),
            "Verify accepted pointee {pointee:?}"
        );
        assert!(
            zil_verify::verify_module_bytes(&wire, &module.target, &Limits::default()).is_err(),
            "Wire Verify accepted pointee {pointee:?}"
        );
    }
}

#[test]
fn g3_4_1_option_core_metadata_is_authenticated_before_types_resolve() {
    let unit = source(HELPER);
    let ast = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &ast).unwrap();
    let core = zil_types::infer_source_closures(&core, &Limits::default()).unwrap();
    let core =
        zil_ct::evaluate_source_constants(&core, &TargetProfile::default(), &Limits::default())
            .unwrap();
    zil_types::check(&core, &TargetProfile::default(), &Limits::default()).unwrap();
    for mutation in 0..12 {
        let mut forged = core.clone();
        match mutation {
            0 => forged.life_declarations[0].identity.package = "user".into(),
            1 => forged.life_declarations[0].id.module.0 = 0,
            2 => forged.life_declarations[0].parameters.clear(),
            3 => forged.life_declarations[0].identity.generation_path.push(0),
            4 => forged.life_instances[0].arguments[0] = LifeNominalActual::Type(Type::I64),
            5 => {
                let LifeNominalActual::Type(Type::Address { mutable, .. }) =
                    &mut forged.life_instances[0].arguments[0]
                else {
                    panic!()
                };
                *mutable = true;
            }
            _ => {
                let Type::BorrowedStorage { life, .. } = &mut forged.life_instances[0].ty else {
                    panic!()
                };
                match mutation {
                    6 => life.retained.clear(),
                    7 => life.retained[0].class = 1,
                    8 => life.retained[0].binder = 1,
                    9 => life.retained[0].path = vec![LifeTypeProjection::TypeArgument(1)],
                    10 => {
                        life.slots[0].path =
                            vec![PlaceProjection::Variant(zil_ir::VariantProjection {
                                variant: 0,
                                index: 0,
                            })]
                    }
                    11 => life.slots[0].binder = 1,
                    _ => unreachable!(),
                }
            }
        }
        let errors =
            zil_types::check(&forged, &TargetProfile::default(), &Limits::default()).unwrap_err();
        assert!(
            errors.iter().all(|d| d.phase == Phase::Types),
            "Core mutation {mutation}: {errors:?}"
        );
    }
}

#[test]
fn g3_4_1_option_actual_none_and_mixed_variants_forward_without_invented_targets() {
    for body in [
        "return Option::<&'s i64>.None();",
        "if flag{return Option::<&'s i64>.None();}return Option::<&'s i64>.Some(&slice[0]);",
    ] {
        let text = format!(
            "fn next<life s>(slice:&'s [i64],flag:bool)->Option<&'s i64>{{{body}}}fn id<life s>(value:Option<&'s i64>)->Option<&'s i64>{{return value;}}fn forward<life s>(slice:&'s [i64],flag:bool)->Option<&'s i64>{{let value=next(slice,flag);return id(value);}}fn main(){{}}"
        );
        let module = typed(&text);
        let plan = accept(&module);
        if body.starts_with("return") {
            for f in &module.functions {
                if f.name == "next" || f.name == "forward" {
                    for block in &f.blocks {
                        if let zil_ir::Terminator::Return(Some(value)) = block.terminator {
                            assert!(
                                plan.functions[f.id.0 as usize].holds[value.0 as usize]
                                    .roots
                                    .is_empty(),
                                "None has a manufactured target"
                            );
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn g3_4_1_option_none_cannot_hide_recursive_ungrounded_some_input() {
    let good = "fn seed<life s>(slice:&'s [i64])->&'s i64{return &slice[0];}fn missing<life s>(slice:&'s [i64])->&'s i64{return seed(slice);}fn next<life s>(slice:&'s [i64],flag:bool)->Option<&'s i64>{if flag{return Option::<&'s i64>.None();}return Option::<&'s i64>.Some(missing(slice));}fn main(){}";
    let positive = typed(good);
    let mut plan = accept(&positive);
    let bad = typed(&good.replace("return seed(slice);", "return missing(slice);"));
    assert!(zil_analyze::analyze(&bad, &Limits::default()).is_err());
    plan.module_fingerprint = bad.fingerprint();
    let wire = zil_ir::wire::encode_module_candidate(&bad, &plan, &Limits::default()).unwrap();
    let (decoded, _) = zil_ir::wire::decode_module_candidate(&wire, &Limits::default()).unwrap();
    assert!(zil_analyze::analyze(&decoded, &Limits::default()).is_err());
    assert!(zil_verify::verify_module(bad.clone(), plan, &bad.target, &Limits::default()).is_err());
    assert!(zil_verify::verify_module_bytes(&wire, &bad.target, &Limits::default()).is_err());
}

#[test]
fn g3_4_1_option_none_join_does_not_hide_a_constructorless_recursive_producer() {
    let good = "fn seed<life s>(slice:&'s [i64])->Option<&'s i64>{return Option::<&'s i64>.Some(&slice[0]);}fn missing<life s>(slice:&'s [i64])->Option<&'s i64>{return seed(slice);}fn next<life s>(slice:&'s [i64],flag:bool)->Option<&'s i64>{var value:Option<&'s i64> = Option::<&'s i64>.None();if flag{value=missing(slice);}return value;}fn main(){}";
    let positive = typed(good);
    let mut plan = accept(&positive);
    let bad = typed(&good.replace("return seed(slice);", "return missing(slice);"));
    assert!(zil_analyze::analyze(&bad, &Limits::default()).is_err());
    plan.module_fingerprint = bad.fingerprint();
    let wire = zil_ir::wire::encode_module_candidate(&bad, &plan, &Limits::default()).unwrap();
    let (decoded, _) = zil_ir::wire::decode_module_candidate(&wire, &Limits::default()).unwrap();
    assert!(zil_analyze::analyze(&decoded, &Limits::default()).is_err());
    assert!(zil_verify::verify_module(bad.clone(), plan, &bad.target, &Limits::default()).is_err());
    assert!(zil_verify::verify_module_bytes(&wire, &bad.target, &Limits::default()).is_err());
}

#[test]
fn g3_4_1_option_none_join_forwarding_does_not_retain_transient_empty_leaves() {
    let text = "fn id<life s>(value:Option<&'s i64>)->Option<&'s i64>{return value;}fn next<life s>(slice:&'s [i64],flag:bool)->Option<&'s i64>{var value:Option<&'s i64> = Option::<&'s i64>.None();if flag{value=Option::<&'s i64>.None();}else{value=Option::<&'s i64>.None();}let forwarded=id(value);return forwarded;}fn main(){}";
    let module = typed(text);
    let plan = accept(&module);
    let f = module.functions.iter().find(|f| f.name == "next").unwrap();
    for block in &f.blocks {
        if let zil_ir::Terminator::Return(Some(value)) = block.terminator {
            assert!(
                plan.functions[f.id.0 as usize].holds[value.0 as usize]
                    .roots
                    .is_empty()
            );
        }
    }
}

#[test]
fn g3_4_1_option_pattern_projects_only_the_actual_some_payload() {
    let module = typed(include_str!("option-borrowed.zil"));
    let plan = accept(&module);
    assert!(
        module
            .functions
            .iter()
            .flat_map(|f| &f.blocks)
            .flat_map(|b| &b.instructions)
            .any(|i| matches!(
                i.kind,
                zil_ir::InstructionKind::ReadVariant {
                    variant: 1,
                    index: 0,
                    ..
                }
            ))
    );
    for case in 0..5 {
        let mut forged = module.clone();
        let mut changed = false;
        for f in &mut forged.functions {
            for block in &mut f.blocks {
                for i in &mut block.instructions {
                    match &mut i.kind {
                        zil_ir::InstructionKind::MakeVariant { variant, fields }
                            if !changed && *variant == 1 && case < 2 =>
                        {
                            if case == 0 {
                                *variant = 2;
                            } else {
                                fields.clear();
                            }
                            changed = true;
                        }
                        zil_ir::InstructionKind::ReadVariant { variant, index, .. }
                            if !changed && (case == 2 || case == 3) =>
                        {
                            if case == 2 {
                                *variant = 0;
                            } else {
                                *index = 1;
                            }
                            changed = true;
                        }
                        zil_ir::InstructionKind::VariantTag { .. } if !changed && case == 4 => {
                            f.values[i.result.unwrap().0 as usize] = Type::I64;
                            changed = true;
                        }
                        _ => {}
                    }
                }
            }
        }
        assert!(changed, "missing source instruction for mutation {case}");
        assert!(
            zil_analyze::analyze(&forged, &Limits::default()).is_err(),
            "mutation {case}"
        );
        let mut claim = plan.clone();
        claim.module_fingerprint = forged.fingerprint();
        let wire =
            zil_ir::wire::encode_module_candidate(&forged, &claim, &Limits::default()).unwrap();
        let (decoded, _) =
            zil_ir::wire::decode_module_candidate(&wire, &Limits::default()).unwrap();
        assert!(
            zil_analyze::analyze(&decoded, &Limits::default()).is_err(),
            "wire mutation {case}"
        );
        assert!(
            zil_verify::verify_module(forged.clone(), claim, &forged.target, &Limits::default())
                .is_err(),
            "mutation {case}"
        );
        assert!(
            zil_verify::verify_module_bytes(&wire, &forged.target, &Limits::default()).is_err(),
            "wire mutation {case}"
        );
    }
}

#[test]
fn g3_4_1_option_known_and_runtime_variants_match_without_invented_targets() {
    for body in [
        "return Option::<&'s i64>.Some(&slice[0]);",
        "if flag{return Option::<&'s i64>.None();}return Option::<&'s i64>.Some(&slice[0]);",
        "return Option::<&'s i64>.None();",
    ] {
        let text = format!(
            "fn next<life s>(slice:&'s [i64],flag:bool)->Option<&'s i64>{{{body}}}fn result(flag:bool)->i64{{let values:[i64;1]=[42];let item=next(&values[..],flag);match item{{Option.Some(value)=>{{return *value}},Option.None()=>{{return 0}}}}}}"
        );
        accept(&typed(&text));
    }
}

#[test]
fn g3_4_1_option_none_match_dead_edge_requires_the_real_tag_comparison() {
    let text = include_str!("option-borrowed.zil").replace(
        "return Option::<&'s i64>.Some(&slice[0]);",
        "return Option::<&'s i64>.None();",
    );
    let module = typed(&text);
    let plan = accept(&module);
    for case in 0..5 {
        let mut forged = module.clone();
        let mut changed = false;
        for f in &mut forged.functions {
            let Some(tag) = f
                .blocks
                .iter()
                .flat_map(|b| &b.instructions)
                .find(|i| matches!(i.kind, zil_ir::InstructionKind::VariantTag { .. }))
                .and_then(|i| i.result)
            else {
                continue;
            };
            let Some((condition, scalar)) =
                f.blocks
                    .iter()
                    .flat_map(|b| &b.instructions)
                    .find_map(|i| match i.kind {
                        zil_ir::InstructionKind::Binary {
                            op: zil_ir::BinaryOp::Equal,
                            left,
                            right,
                        } if left == tag => Some((i.result.unwrap(), right)),
                        zil_ir::InstructionKind::Binary {
                            op: zil_ir::BinaryOp::Equal,
                            left,
                            right,
                        } if right == tag => Some((i.result.unwrap(), left)),
                        _ => None,
                    })
            else {
                continue;
            };
            for block in &mut f.blocks {
                for i in &mut block.instructions {
                    if case == 0 && i.result == Some(tag) {
                        i.kind = zil_ir::InstructionKind::Constant(zil_ir::Constant::I32(0));
                        changed = true;
                    }
                    if (case == 1 || case == 3)
                        && i.result == Some(condition)
                        && let zil_ir::InstructionKind::Binary { op, left, right } = &mut i.kind
                    {
                        if case == 1 {
                            *op = zil_ir::BinaryOp::NotEqual;
                        } else {
                            *left = scalar;
                            *right = scalar;
                        }
                        changed = true;
                    }
                    if case == 4 && i.result == Some(scalar) {
                        i.kind = zil_ir::InstructionKind::Constant(zil_ir::Constant::I32(0));
                        changed = true;
                    }
                }
                if case == 2
                    && let zil_ir::Terminator::Branch {
                        condition: actual,
                        then_block,
                        else_block,
                    } = &mut block.terminator
                    && *actual == condition
                {
                    std::mem::swap(then_block, else_block);
                    changed = true;
                }
            }
        }
        assert!(changed, "missing source tag/condition {case}");
        assert!(
            zil_analyze::analyze(&forged, &Limits::default()).is_err(),
            "condition {case}"
        );
        let mut claim = plan.clone();
        claim.module_fingerprint = forged.fingerprint();
        let wire =
            zil_ir::wire::encode_module_candidate(&forged, &claim, &Limits::default()).unwrap();
        let (decoded, _) =
            zil_ir::wire::decode_module_candidate(&wire, &Limits::default()).unwrap();
        assert!(
            zil_analyze::analyze(&decoded, &Limits::default()).is_err(),
            "wire condition {case}"
        );
        assert!(
            zil_verify::verify_module(forged.clone(), claim, &forged.target, &Limits::default())
                .is_err(),
            "condition {case}"
        );
        assert!(
            zil_verify::verify_module_bytes(&wire, &forged.target, &Limits::default()).is_err(),
            "wire condition {case}"
        );
    }
}

#[test]
fn g3_4_1_option_none_to_some_backedge_keeps_the_runtime_some_arm() {
    let text = "fn spin<life s>(slice:&'s [i64])->i64{var item:Option<&'s i64> = Option::<&'s i64>.None();while true{match item{Option.None()=>{item=Option::<&'s i64>.Some(&slice[0]);continue},Option.Some(value)=>{return *value}}}}fn main(){}";
    let module = typed(text);
    let plan = accept(&module);
    let f = module.functions.iter().find(|f| f.name == "spin").unwrap();
    let mut checked = false;
    for i in f.blocks.iter().flat_map(|b| &b.instructions) {
        if matches!(
            i.kind,
            zil_ir::InstructionKind::ReadVariant {
                variant: 1,
                index: 0,
                ..
            }
        ) {
            assert!(
                plan.functions[f.id.0 as usize].holds[i.result.unwrap().0 as usize]
                    .roots
                    .contains(&HoldRoot::Parameter(0))
            );
            checked = true;
        }
    }
    assert!(
        checked,
        "source loop did not contain the actual Some projection"
    );
}
