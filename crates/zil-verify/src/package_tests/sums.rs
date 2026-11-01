//! Independent raw and Wire probes. Neither Types nor Analyze is called.
use super::*;

fn sum() -> Type {
    Type::Enum {
        name: "OwnedPair".into(),
        variants: vec![
            ("Pair".into(), vec![owned(), owned(), Type::I32]),
            ("Empty".into(), vec![]),
            ("Copy".into(), vec![Type::I32]),
        ],
    }
}
fn pair(b: &mut Body) -> (LocalId, [LocalId; 3]) {
    let source = b.storage(sum());
    let targets = [b.storage(owned()), b.storage(owned()), b.storage(Type::I32)];
    let a = b.token(65);
    let c = b.token(66);
    let n = b.value(Type::I32, InstructionKind::Constant(Constant::I32(7)));
    let value = b.value(
        sum(),
        InstructionKind::MakeVariant {
            variant: 0,
            fields: vec![a, c, n],
        },
    );
    b.op(InstructionKind::Store {
        local: source,
        value,
    });
    (source, targets)
}
fn unpack(b: &mut Body, source: LocalId, targets: &[LocalId]) {
    b.op(InstructionKind::UnpackVariant {
        local: source,
        variant: 0,
        fields: targets.to_vec(),
    });
}
fn pair_module() -> TypedModule {
    let mut b = Body::new(Type::Unit);
    let (source, targets) = pair(&mut b);
    unpack(&mut b, source, &targets);
    b.op(InstructionKind::DropEnum { local: source }); // live moved temp: flag no-op
    b.op(InstructionKind::DropManaged { local: targets[0] });
    b.op(InstructionKind::DropManaged { local: targets[1] });
    module(b.finish(None, CleanupExitKind::Return))
}
fn reject_semantic(module: TypedModule, needle: &str) {
    let (raw, wire) = check_both(module);
    for result in [raw, wire] {
        let error = result.unwrap_err();
        assert!(
            format!("{error:?}").contains(needle),
            "expected {needle}: {error:?}"
        );
        assert!(
            !matches!(error[0].code.as_str(), "E-VERIFY-PLAN" | "E-VERIFY-HOLD"),
            "candidate-only rejection: {error:?}"
        );
    }
}

#[test]
fn owned_sum_atomic_multiple_payloads_and_moved_conditional_drop() {
    accepted(pair_module());
}

#[test]
fn owned_sum_unit_and_copy_active_variants_are_consumed_once() {
    for variant in [1, 2] {
        let mut b = Body::new(Type::Unit);
        let source = b.storage(sum());
        let mut fields = vec![];
        let mut targets = vec![];
        if variant == 2 {
            fields.push(b.value(Type::I32, InstructionKind::Constant(Constant::I32(9))));
            targets.push(b.storage(Type::I32));
        }
        let value = b.value(sum(), InstructionKind::MakeVariant { variant, fields });
        b.op(InstructionKind::Store {
            local: source,
            value,
        });
        b.op(InstructionKind::UnpackVariant {
            local: source,
            variant,
            fields: targets,
        });
        b.op(InstructionKind::DropEnum { local: source });
        accepted(module(b.finish(None, CleanupExitKind::Return)));
    }
}

#[test]
fn owned_sum_rejects_wrong_tag_duplicate_targets_and_initialized_copy_target() {
    let mut m = pair_module();
    let op = m.functions[0].blocks[0]
        .instructions
        .iter_mut()
        .find(|i| matches!(i.kind, InstructionKind::UnpackVariant { .. }))
        .unwrap();
    if let InstructionKind::UnpackVariant {
        variant, fields, ..
    } = &mut op.kind
    {
        *variant = 1;
        fields.clear();
    }
    reject_semantic(m, "exact selected tag");
    let mut m = pair_module();
    if let InstructionKind::UnpackVariant { fields, .. } = &mut m.functions[0].blocks[0]
        .instructions
        .iter_mut()
        .find(|i| matches!(i.kind, InstructionKind::UnpackVariant { .. }))
        .unwrap()
        .kind
    {
        fields[1] = fields[0];
    }
    reject_semantic(m, "distinct");
    let mut m = pair_module();
    let f = &mut m.functions[0];
    let index = f.blocks[0]
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::UnpackVariant { .. }))
        .unwrap();
    let value = ValueId(f.values.len() as u32);
    f.values.push(Type::I32);
    f.blocks[0].instructions.splice(
        index..index,
        [
            ins(Some(value), InstructionKind::Constant(Constant::I32(1))),
            ins(
                None,
                InstructionKind::Store {
                    local: LocalId(3),
                    value,
                },
            ),
        ],
    );
    reject_semantic(m, "vacant");
}

fn tested_parameter() -> TypedModule {
    let mut b = Body::new(Type::Unit);
    let source = b.parameter(sum());
    let targets = [b.storage(owned()), b.storage(owned()), b.storage(Type::I32)];
    let tag = b.value(
        Type::I32,
        InstructionKind::VariantTagLocal {
            local: source,
            path: vec![],
        },
    );
    let zero = b.value(Type::I32, InstructionKind::Constant(Constant::I32(0)));
    let test = b.value(
        Type::Bool,
        InstructionKind::Binary {
            op: BinaryOp::Equal,
            left: tag,
            right: zero,
        },
    );
    let split = b.0.blocks[0].instructions.len();
    unpack(&mut b, source, &targets);
    b.op(InstructionKind::DropManaged { local: targets[0] });
    b.op(InstructionKind::DropManaged { local: targets[1] });
    let mut f = b.finish(None, CleanupExitKind::Return);
    let tail = f.blocks[0].instructions.split_off(split);
    f.blocks[0].terminator = Terminator::Branch {
        condition: test,
        then_block: BlockId(1),
        else_block: BlockId(2),
    };
    f.blocks.push(BasicBlock {
        id: BlockId(1),
        instructions: tail,
        terminator: Terminator::Return(None),
    });
    f.blocks.push(BasicBlock {
        id: BlockId(2),
        instructions: vec![],
        terminator: Terminator::Trap,
    });
    module(f)
}

#[test]
fn owned_sum_exact_tag_edge_accepts_but_opposite_and_bypass_do_not() {
    accepted(tested_parameter());
    let mut m = tested_parameter();
    if let Terminator::Branch {
        then_block,
        else_block,
        ..
    } = &mut m.functions[0].blocks[0].terminator
    {
        std::mem::swap(then_block, else_block);
    }
    reject_semantic(m, "exact selected tag");
    let mut m = tested_parameter();
    m.functions[0].blocks[2].terminator = Terminator::Jump(BlockId(1));
    reject_semantic(m, "exact selected tag");
    let mut m = tested_parameter();
    if let Terminator::Branch { else_block, .. } = &mut m.functions[0].blocks[0].terminator {
        *else_block = BlockId(1);
    }
    reject_semantic(m, "exact selected tag");
}

#[test]
fn owned_sum_stale_tag_after_take_store_cannot_reauthorize_current_local() {
    let mut m = tested_parameter();
    let f = &mut m.functions[0];
    let index = f.blocks[0]
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::VariantTagLocal { .. }))
        .unwrap()
        + 1;
    let value = ValueId(f.values.len() as u32);
    f.values.push(sum());
    f.blocks[0].instructions.splice(
        index..index,
        [
            ins(
                Some(value),
                InstructionKind::TakeLocal { local: LocalId(0) },
            ),
            ins(
                None,
                InstructionKind::Store {
                    local: LocalId(0),
                    value,
                },
            ),
        ],
    );
    reject_semantic(m, "exact selected tag");
}

#[test]
fn owned_sum_rejects_copy_ssa_view_and_true_double_drop() {
    let mut m = pair_module();
    let f = &mut m.functions[0];
    let index = f.blocks[0]
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::UnpackVariant { .. }))
        .unwrap();
    let value = ValueId(f.values.len() as u32);
    f.values.push(sum());
    f.blocks[0]
        .instructions
        .insert(index, ins(Some(value), InstructionKind::Load(LocalId(0))));
    reject_semantic(m, "never copying Load");
    let mut b = Body::new(Type::Unit);
    let (source, _) = pair(&mut b);
    b.op(InstructionKind::DropEnum { local: source });
    b.op(InstructionKind::DropEnum { local: source });
    reject_semantic(
        module(b.finish(None, CleanupExitKind::Return)),
        "duplicate automatic cleanup",
    );
}

fn fresh_field(swapped: bool, relay: bool) -> TypedModule {
    let mut b = Body::new(owned());
    b.0.result_mode = PassingMode::Create;
    let input = b.parameter(owned());
    let source = b.storage(sum());
    let targets = [b.storage(owned()), b.storage(owned()), b.storage(Type::I32)];
    let incoming = b.value(owned(), InstructionKind::TakeLocal { local: input });
    let fresh = b.token(70);
    let n = b.value(Type::I32, InstructionKind::Constant(Constant::I32(7)));
    let mut packed = b.value(
        sum(),
        InstructionKind::MakeVariant {
            variant: 0,
            fields: vec![incoming, fresh, n],
        },
    );
    if relay {
        packed = b.value(
            sum(),
            InstructionKind::Call {
                function: FunctionId(1),
                arguments: vec![packed],
            },
        );
    }
    b.op(InstructionKind::Store {
        local: source,
        value: packed,
    });
    let order = if swapped {
        [targets[1], targets[0], targets[2]]
    } else {
        targets
    };
    unpack(&mut b, source, &order);
    let result = b.value(owned(), InstructionKind::TakeLocal { local: targets[1] });
    b.op(InstructionKind::DropManaged { local: targets[0] });
    let mut m = module(b.finish(Some(result), CleanupExitKind::Return));
    if relay {
        let mut body = Body::new(sum());
        body.0.id = FunctionId(1);
        body.0.name = "relay".into();
        let local = body.parameter(sum());
        let result = body.value(sum(), InstructionKind::TakeLocal { local });
        m.functions
            .push(body.finish(Some(result), CleanupExitKind::Return));
    }
    m
}

#[test]
fn owned_sum_deep_freshness_survives_pack_unpack_and_function_forwarding() {
    accepted(fresh_field(false, false));
    accepted(fresh_field(false, true));
    reject_semantic(fresh_field(true, false), "not independently proven fresh");
    reject_semantic(fresh_field(true, true), "not independently proven fresh");
}

fn right_nested(publish: bool) -> TypedModule {
    let outer = Type::Enum {
        name: "Outer".into(),
        variants: vec![
            ("Pair".into(), vec![owned(), sum()]),
            ("Empty".into(), vec![]),
        ],
    };
    let mut b = Body::new(Type::Unit);
    let root = b.storage(outer.clone());
    let a = b.storage(owned());
    let inner = b.storage(sum());
    let c = b.storage(owned());
    let d = b.storage(owned());
    let n = b.storage(Type::I32);
    let finals = [b.storage(owned()), b.storage(owned()), b.storage(owned())];
    let av = b.token(65);
    let cv = b.token(67);
    let dv = b.token(68);
    let number = b.value(Type::I32, InstructionKind::Constant(Constant::I32(5)));
    let iv = b.value(
        sum(),
        InstructionKind::MakeVariant {
            variant: 0,
            fields: vec![cv, dv, number],
        },
    );
    let value = b.value(
        outer,
        InstructionKind::MakeVariant {
            variant: 0,
            fields: vec![av, iv],
        },
    );
    b.op(InstructionKind::Store { local: root, value });
    b.value(
        Type::I32,
        InstructionKind::VariantTagLocal {
            local: root,
            path: vec![VariantProjection {
                variant: 0,
                index: 1,
            }],
        },
    );
    b.value(
        Type::I32,
        InstructionKind::ReadVariantLocal {
            local: root,
            path: vec![
                VariantProjection {
                    variant: 0,
                    index: 1,
                },
                VariantProjection {
                    variant: 0,
                    index: 2,
                },
            ],
        },
    );
    unpack(&mut b, root, &[a, inner]);
    unpack(&mut b, inner, &[c, d, n]);
    if publish {
        for (from, to) in [(d, finals[2]), (c, finals[1]), (a, finals[0])] {
            let value = b.value(owned(), InstructionKind::TakeLocal { local: from });
            b.op(InstructionKind::Store { local: to, value });
        }
        for local in finals {
            b.op(InstructionKind::DropManaged { local });
        }
    } else {
        for local in [a, c, d] {
            b.op(InstructionKind::DropManaged { local });
        }
    }
    b.op(InstructionKind::DropEnum { local: inner });
    b.op(InstructionKind::DropEnum { local: root });
    module(b.finish(None, CleanupExitKind::Return))
}

#[test]
fn owned_sum_right_nested_pattern_uses_real_reverse_dfs_publication() {
    accepted(right_nested(true));
    reject_semantic(right_nested(false), "reverse successful initialization");
}

#[test]
fn owned_sum_typed_paths_reject_non_enum_steps_owner_endpoints_and_missing_prefix_tags() {
    let mut m = right_nested(true);
    if let InstructionKind::ReadVariantLocal { path, .. } = &mut m.functions[0].blocks[0]
        .instructions
        .iter_mut()
        .find(|i| matches!(i.kind, InstructionKind::ReadVariantLocal { .. }))
        .unwrap()
        .kind
    {
        path[0].index = 0;
    }
    reject_semantic(m, "only traverse an actual Enum");
    let mut m = right_nested(true);
    if let InstructionKind::ReadVariantLocal { path, .. } = &mut m.functions[0].blocks[0]
        .instructions
        .iter_mut()
        .find(|i| matches!(i.kind, InstructionKind::ReadVariantLocal { .. }))
        .unwrap()
        .kind
    {
        path.pop();
    }
    reject_semantic(m, "finite Copy");
    let mut m = tested_parameter();
    let f = &mut m.functions[0];
    let value = ValueId(f.values.len() as u32);
    f.values.push(Type::I32);
    let index = f.blocks[0]
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::VariantTagLocal { .. }))
        .unwrap();
    f.blocks[0].instructions.insert(
        index,
        ins(
            Some(value),
            InstructionKind::ReadVariantLocal {
                local: LocalId(0),
                path: vec![VariantProjection {
                    variant: 0,
                    index: 2,
                }],
            },
        ),
    );
    reject_semantic(m, "exact selected tag");
}

#[test]
fn owned_sum_budget_n_minus_one_and_fresh_retry() {
    let m = right_nested(true);
    let p = resource_tests::plan(&m);
    let mut low = 0;
    let mut high = Limits::default().max_analysis_steps;
    while low + 1 < high {
        let middle = low + (high - low) / 2;
        let mut limits = Limits::default();
        limits.max_analysis_steps = middle;
        if verify_module(m.clone(), p.clone(), &m.target, &limits).is_ok() {
            high = middle;
        } else {
            low = middle;
        }
    }
    let mut limits = Limits::default();
    limits.max_analysis_steps = high;
    verify_module(m.clone(), p.clone(), &m.target, &limits).unwrap();
    limits.max_analysis_steps -= 1;
    assert_eq!(
        verify_module(m.clone(), p.clone(), &m.target, &limits).unwrap_err()[0].category,
        zil_base::ResultCategory::ResourceLimit
    );
    limits.max_analysis_steps += 1;
    verify_module(m.clone(), p, &m.target, &limits).unwrap();
    eprintln!("242 independent Verify right-nested N={high}");
}

#[test]
fn owned_sum_copy_local_observers_and_exact_request_limits_are_retained() {
    let mut b = Body::new(Type::Unit);
    let ty = Type::Enum {
        name: "CopyE".into(),
        variants: vec![("Some".into(), vec![Type::I32])],
    };
    let local = b.storage(ty.clone());
    let n = b.value(Type::I32, InstructionKind::Constant(Constant::I32(4)));
    let value = b.value(
        ty,
        InstructionKind::MakeVariant {
            variant: 0,
            fields: vec![n],
        },
    );
    b.op(InstructionKind::Store { local, value });
    b.value(
        Type::I32,
        InstructionKind::VariantTagLocal {
            local,
            path: vec![],
        },
    );
    b.value(
        Type::I32,
        InstructionKind::ReadVariantLocal {
            local,
            path: vec![VariantProjection {
                variant: 0,
                index: 0,
            }],
        },
    );
    let m = module(b.finish(None, CleanupExitKind::Return));
    let p = resource_tests::plan(&m);
    let mut limits = Limits::default();
    limits.max_analysis_steps -= 3;
    limits.max_call_depth -= 1;
    let shared_m = Arc::new(m.clone());
    let shared_p = Arc::new(p.clone());
    let raw = verify_module(m.clone(), p.clone(), &m.target, &limits).unwrap();
    let shared = verify_shared_module(shared_m, shared_p, &m.target, &limits).unwrap();
    let program = Arc::new(TypedProgram {
        module: m.clone(),
        interfaces: vec![],
        root_functions: vec![],
    });
    let shared_program =
        verify_shared_program_module(program, Arc::new(p.clone()), &m.target, &limits).unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&m, &p, &limits).unwrap();
    let replay = verify_module_bytes(&bytes, &m.target, &limits).unwrap();
    for verified in [raw.clone(), raw, shared, shared_program, replay] {
        assert_eq!(verified.limits(), &limits);
        assert!(verified.uses_owned_sum_extension());
    }
    let old = verify_module_bytes(&bytes, &m.target, &limits).unwrap();
    limits.max_analysis_steps -= 7;
    let current = verify_module_bytes(&bytes, &m.target, &limits).unwrap();
    assert_ne!(old.limits(), current.limits());
    assert_eq!(current.limits(), &limits);
    let legacy = plain();
    let lp = resource_tests::plan(&legacy);
    let legacy = verify_module(legacy, lp, &m.target, &limits).unwrap();
    assert!(!legacy.uses_owned_sum_extension());
    accepted(m);
}
