//! Producer-only raw/Wire probes; no Types or Verify permission is imported.
use super::*;
use zil_base::{Fingerprint, ModuleId, ResultCategory, Span, TargetProfile};
fn op(result: Option<ValueId>, kind: InstructionKind) -> Instruction {
    Instruction {
        result,
        kind,
        span: Span::default(),
    }
}
fn payload() -> Type {
    Type::Record {
        name: "Token".into(),
        fields: vec![("n".into(), Type::I32)],
    }
}
fn leaf() -> Type {
    Type::Managed(Box::new(payload()))
}
fn sum() -> Type {
    Type::Enum {
        name: "Event".into(),
        variants: vec![
            ("Pair".into(), vec![leaf(), leaf()]),
            ("Empty".into(), vec![]),
        ],
    }
}
fn module(parameter: bool, swap: bool) -> TypedModule {
    let ty = sum();
    let mut f = TypedFunction {
        callable_return_contract: None,
        callable_contract: None,
        id: FunctionId(0),
        name: "main".into(),
        foreign: None,
        environment: None,
        parameters: if parameter { vec![LocalId(0)] } else { vec![] },
        return_type: Type::Unit,
        result_mode: PassingMode::Infer,
        locals: vec![],
        values: vec![],
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![op(None, InstructionKind::ScopeEnter { scope: ScopeId(0) })],
            terminator: Terminator::Return(None),
        }],
        entry: BlockId(0),
        span: Span::default(),
    };
    for (id, ty) in [ty.clone(), leaf(), leaf()].into_iter().enumerate() {
        let id = LocalId(id as u32);
        f.locals.push(LocalDecl {
            id,
            name: format!("l{}", id.0),
            ty,
            mutable: true,
            span: Span::default(),
        });
        if !parameter || id.0 != 0 {
            f.blocks[0]
                .instructions
                .push(op(None, InstructionKind::StorageLive(id)));
        }
    }
    if !parameter {
        f.values = vec![Type::I32, leaf(), leaf(), ty.clone()];
        f.blocks[0].instructions.extend([
            op(
                Some(ValueId(0)),
                InstructionKind::Constant(Constant::I32(1)),
            ),
            op(
                Some(ValueId(1)),
                InstructionKind::MakeManaged {
                    fields: vec![ValueId(0)],
                },
            ),
            op(
                Some(ValueId(2)),
                InstructionKind::MakeManaged {
                    fields: vec![ValueId(0)],
                },
            ),
            op(
                Some(ValueId(3)),
                InstructionKind::MakeVariant {
                    variant: 0,
                    fields: vec![ValueId(1), ValueId(2)],
                },
            ),
            op(
                None,
                InstructionKind::Store {
                    local: LocalId(0),
                    value: ValueId(3),
                },
            ),
        ]);
    }
    let fields = if swap {
        vec![LocalId(2), LocalId(1)]
    } else {
        vec![LocalId(1), LocalId(2)]
    };
    let mut cleanup = vec![op(
        None,
        InstructionKind::UnpackVariant {
            local: LocalId(0),
            variant: 0,
            fields: fields.clone(),
        },
    )];
    for l in &fields {
        cleanup.push(op(None, InstructionKind::DropManaged { local: *l }));
    }
    for id in [2, 1, 0] {
        cleanup.push(op(None, InstructionKind::StorageDead(LocalId(id))));
    }
    cleanup.push(op(
        None,
        InstructionKind::ScopeExit {
            scope: ScopeId(0),
            kind: CleanupExitKind::Return,
        },
    ));
    if parameter {
        f.values = vec![Type::I32, Type::I32, Type::Bool];
        f.blocks[0].instructions.extend([
            op(
                Some(ValueId(0)),
                InstructionKind::VariantTagLocal {
                    local: LocalId(0),
                    path: vec![],
                },
            ),
            op(
                Some(ValueId(1)),
                InstructionKind::Constant(Constant::I32(0)),
            ),
            op(
                Some(ValueId(2)),
                InstructionKind::Binary {
                    op: BinaryOp::Equal,
                    left: ValueId(0),
                    right: ValueId(1),
                },
            ),
        ]);
        f.blocks[0].terminator = Terminator::Branch {
            condition: ValueId(2),
            then_block: BlockId(1),
            else_block: BlockId(2),
        };
        f.blocks.push(BasicBlock {
            id: BlockId(1),
            instructions: cleanup,
            terminator: Terminator::Return(None),
        });
        let mut other = vec![op(None, InstructionKind::DropEnum { local: LocalId(0) })];
        for id in [2, 1, 0] {
            other.push(op(None, InstructionKind::StorageDead(LocalId(id))));
        }
        other.push(op(
            None,
            InstructionKind::ScopeExit {
                scope: ScopeId(0),
                kind: CleanupExitKind::Return,
            },
        ));
        f.blocks.push(BasicBlock {
            id: BlockId(2),
            instructions: other,
            terminator: Terminator::Return(None),
        });
    } else {
        f.blocks[0].instructions.extend(cleanup);
    }
    TypedModule {
        schema_version: zil_base::SCHEMA_VERSION,
        module_id: ModuleId(0),
        source_fingerprint: Fingerprint::ZERO,
        target: TargetProfile::default(),
        functions: vec![f],
        closures: vec![],
        resources: vec![ResourceDefinition {
            name: "Token".into(),
            payload: payload(),
            teardown: None,
        }],
        unsupported: vec![],
    }
}
fn both(m: &TypedModule) -> Vec<CompileResult<CandidatePlan>> {
    let limits = Limits::default();
    let bytes =
        zil_ir::wire::encode_module_candidate(m, &CandidatePlan::empty_for(m), &limits).unwrap();
    let decoded = zil_ir::wire::decode_module_candidate(&bytes, &limits)
        .unwrap()
        .0;
    vec![
        analyze(m, &Limits::default()),
        analyze(&decoded, &Limits::default()),
    ]
}
#[test]
fn constructed_multifield_and_actual_same_type_target_mapping() {
    for swap in [false, true] {
        for result in both(&module(false, swap)) {
            result.unwrap();
        }
    }
}
#[test]
fn branch_success_proves_current_variant_and_other_edge_drops_whole() {
    for result in both(&module(true, false)) {
        result.unwrap();
    }
}
#[test]
fn unguarded_unknown_and_opposite_edge_are_rejected() {
    let mut m = module(true, false);
    m.functions[0].blocks[0].terminator = Terminator::Jump(BlockId(1));
    for r in both(&m) {
        assert!(
            r.unwrap_err()
                .iter()
                .any(|d| d.message.contains("selected"))
        );
    }
    let mut m = module(true, false);
    if let Terminator::Branch {
        then_block,
        else_block,
        ..
    } = &mut m.functions[0].blocks[0].terminator
    {
        std::mem::swap(then_block, else_block);
    }
    assert!(analyze(&m, &Limits::default()).is_err());
}
#[test]
fn unpack_atomic_target_identity_vacancy_type_and_arity_are_checked() {
    for mode in 0..5 {
        let mut m = module(false, false);
        let i = m.functions[0].blocks[0]
            .instructions
            .iter_mut()
            .find(|i| matches!(i.kind, InstructionKind::UnpackVariant { .. }))
            .unwrap();
        if let InstructionKind::UnpackVariant {
            fields, variant, ..
        } = &mut i.kind
        {
            match mode {
                0 => fields[0] = LocalId(0),
                1 => fields[1] = fields[0],
                2 => fields.pop().map(|_| ()).unwrap(),
                3 => *variant = 1,
                _ => fields[0] = LocalId(99),
            }
        }
        for r in both(&m) {
            assert!(r.is_err(), "mutation {mode}");
        }
    }
}
#[test]
fn source_cannot_be_observed_or_unpacked_twice() {
    for mode in 0..2 {
        let mut m = module(false, false);
        let f = &mut m.functions[0];
        let at = f.blocks[0]
            .instructions
            .iter()
            .position(|i| matches!(i.kind, InstructionKind::UnpackVariant { .. }))
            .unwrap()
            + 1;
        let instruction = if mode == 0 {
            op(
                None,
                InstructionKind::UnpackVariant {
                    local: LocalId(0),
                    variant: 0,
                    fields: vec![LocalId(1), LocalId(2)],
                },
            )
        } else {
            let v = ValueId(f.values.len() as u32);
            f.values.push(Type::I32);
            op(
                Some(v),
                InstructionKind::VariantTagLocal {
                    local: LocalId(0),
                    path: vec![],
                },
            )
        };
        f.blocks[0].instructions.insert(at, instruction);
        for result in both(&m) {
            assert!(result.is_err());
        }
    }
    // Conditional cleanup of a moved temporary keeps the established flag ABI.
    let mut m = module(false, false);
    let at = m.functions[0].blocks[0]
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::UnpackVariant { .. }))
        .unwrap()
        + 1;
    m.functions[0].blocks[0].instructions.insert(
        at,
        op(None, InstructionKind::DropEnum { local: LocalId(0) }),
    );
    analyze(&m, &Limits::default()).unwrap();
}
#[test]
fn incoming_leaf_cannot_be_laundered_to_create_through_variant() {
    let mut m = module(true, false);
    let f = &mut m.functions[0];
    f.return_type = leaf();
    f.result_mode = PassingMode::Create;
    let block = &mut f.blocks[1];
    block
        .instructions
        .retain(|i| !matches!(i.kind, InstructionKind::DropManaged { local: LocalId(1) }));
    let take = ValueId(f.values.len() as u32);
    f.values.push(leaf());
    block.instructions.insert(
        1,
        op(Some(take), InstructionKind::TakeLocal { local: LocalId(1) }),
    );
    block.terminator = Terminator::Return(Some(take));
    assert!(
        analyze(&m, &Limits::default())
            .unwrap_err()
            .iter()
            .any(|d| d.message.contains("create result"))
    );
}
#[test]
fn sum_budget_boundary_fresh_and_incremental_are_exact() {
    let m = module(true, false);
    let mut lo = 0;
    let mut hi = Limits::default().max_analysis_steps;
    while lo + 1 < hi {
        let n = (lo + hi) / 2;
        let mut l = Limits::default();
        l.max_analysis_steps = n;
        if analyze(&m, &l).is_ok() {
            hi = n
        } else {
            lo = n
        }
    }
    eprintln!("impl241 Analyze exact boundary N={hi}");
    let mut l = Limits::default();
    l.max_analysis_steps = hi;
    analyze(&m, &l).unwrap();
    let mut session = AnalysisSession::default();
    session.analyze(&m, &l).unwrap();
    session.analyze(&m, &l).unwrap();
    l.max_analysis_steps = hi - 1;
    assert_eq!(
        analyze(&m, &l).unwrap_err()[0].category,
        ResultCategory::ResourceLimit
    );
    assert!(session.analyze(&m, &l).is_err());
    analyze(&m, &Limits::default()).unwrap();
}
#[test]
fn unit_variant_remains_affine_and_zero_field_unpack_consumes_it() {
    let mut m = module(false, false);
    let f = &mut m.functions[0];
    f.blocks[0].instructions.retain(|i| {
        !matches!(
            i.kind,
            InstructionKind::MakeManaged { .. } | InstructionKind::DropManaged { .. }
        )
    });
    for i in &mut f.blocks[0].instructions {
        match &mut i.kind {
            InstructionKind::MakeVariant { variant, fields } => {
                *variant = 1;
                fields.clear()
            }
            InstructionKind::UnpackVariant {
                variant, fields, ..
            } => {
                *variant = 1;
                fields.clear()
            }
            _ => {}
        }
    }
    for r in both(&m) {
        r.unwrap();
    }
    let f = &mut m.functions[0];
    let at = f.blocks[0]
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::UnpackVariant { .. }))
        .unwrap();
    f.blocks[0].instructions.insert(
        at + 1,
        op(
            None,
            InstructionKind::UnpackVariant {
                local: LocalId(0),
                variant: 1,
                fields: vec![],
            },
        ),
    );
    assert!(analyze(&m, &Limits::default()).is_err());
}
#[test]
fn nested_enum_projection_and_unpack_preserve_created_leaf() {
    let mut m = module(false, false);
    let f = &mut m.functions[0];
    let inner = Type::Enum {
        name: "Inner".into(),
        variants: vec![
            ("One".into(), vec![leaf(), Type::I32]),
            ("Empty".into(), vec![]),
        ],
    };
    let outer = Type::Enum {
        name: "Outer".into(),
        variants: vec![
            ("Pair".into(), vec![inner.clone(), leaf()]),
            ("Empty".into(), vec![]),
        ],
    };
    f.locals[0].ty = outer.clone();
    f.locals[1].ty = inner.clone();
    f.values[3] = outer;
    let inner_value = ValueId(f.values.len() as u32);
    f.values.push(inner);
    let at = f.blocks[0]
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::MakeVariant { .. }))
        .unwrap();
    f.blocks[0].instructions.insert(
        at,
        op(
            Some(inner_value),
            InstructionKind::MakeVariant {
                variant: 0,
                fields: vec![ValueId(1), ValueId(0)],
            },
        ),
    );
    if let InstructionKind::MakeVariant { fields, .. } = &mut f.blocks[0].instructions[at + 1].kind
    {
        fields[0] = inner_value;
    }
    for (id, ty) in [(3, leaf()), (4, Type::I32)] {
        f.locals.push(LocalDecl {
            id: LocalId(id),
            name: format!("l{id}"),
            ty,
            mutable: true,
            span: Span::default(),
        });
        f.blocks[0]
            .instructions
            .insert(1, op(None, InstructionKind::StorageLive(LocalId(id))));
    }
    let at = f.blocks[0]
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::UnpackVariant { .. }))
        .unwrap();
    let read = ValueId(f.values.len() as u32);
    f.values.push(Type::I32);
    f.blocks[0].instructions.insert(
        at,
        op(
            Some(read),
            InstructionKind::ReadVariantLocal {
                local: LocalId(0),
                path: vec![
                    VariantProjection {
                        variant: 0,
                        index: 0,
                    },
                    VariantProjection {
                        variant: 0,
                        index: 1,
                    },
                ],
            },
        ),
    );
    let at = f.blocks[0]
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::DropManaged { local: LocalId(1) }))
        .unwrap();
    f.blocks[0].instructions[at] = op(
        None,
        InstructionKind::UnpackVariant {
            local: LocalId(1),
            variant: 0,
            fields: vec![LocalId(3), LocalId(4)],
        },
    );
    f.blocks[0].instructions.insert(
        at + 1,
        op(None, InstructionKind::DropManaged { local: LocalId(3) }),
    );
    let at = f.blocks[0]
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::ScopeExit { .. }))
        .unwrap();
    for l in [3, 4] {
        f.blocks[0]
            .instructions
            .insert(at, op(None, InstructionKind::StorageDead(LocalId(l))));
    }
    for r in both(&m) {
        r.unwrap();
    }
    // Moving this newly created leaf out through two enum layers keeps Created.
    let f = &mut m.functions[0];
    f.return_type = leaf();
    f.result_mode = PassingMode::Create;
    let at = f.blocks[0]
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::DropManaged { local: LocalId(3) }))
        .unwrap();
    let out = ValueId(f.values.len() as u32);
    f.values.push(leaf());
    f.blocks[0].instructions[at] = op(Some(out), InstructionKind::TakeLocal { local: LocalId(3) });
    f.blocks[0].terminator = Terminator::Return(Some(out));
    for r in both(&m) {
        r.unwrap();
    }
}
#[test]
fn stale_tag_snapshot_does_not_authorize_replaced_parameter() {
    let mut m = module(true, false);
    let f = &mut m.functions[0];
    for id in [3, 4] {
        f.locals.push(LocalDecl {
            id: LocalId(id),
            name: format!("l{id}"),
            ty: sum(),
            mutable: true,
            span: Span::default(),
        });
    }
    f.parameters.push(LocalId(3));
    f.blocks[0]
        .instructions
        .insert(1, op(None, InstructionKind::StorageLive(LocalId(4))));
    let save = ValueId(f.values.len() as u32);
    f.values.push(sum());
    let replacement = ValueId(f.values.len() as u32);
    f.values.push(sum());
    // Old observation executes first. Both saved and incoming replacement remain
    // ordinary real values, and the latter has no construction-tag shortcut.
    let at = f.blocks[0]
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::Constant(_)))
        .unwrap();
    f.blocks[0].instructions.splice(
        at..at,
        [
            op(Some(save), InstructionKind::TakeLocal { local: LocalId(0) }),
            op(
                None,
                InstructionKind::Store {
                    local: LocalId(4),
                    value: save,
                },
            ),
            op(
                Some(replacement),
                InstructionKind::TakeLocal { local: LocalId(3) },
            ),
            op(
                None,
                InstructionKind::Store {
                    local: LocalId(0),
                    value: replacement,
                },
            ),
        ],
    );
    for b in &mut f.blocks[1..] {
        let at = b
            .instructions
            .iter()
            .position(|i| matches!(i.kind, InstructionKind::StorageDead(_)))
            .unwrap();
        b.instructions.insert(
            at,
            op(None, InstructionKind::DropEnum { local: LocalId(4) }),
        );
        let at = b
            .instructions
            .iter()
            .position(|i| matches!(i.kind, InstructionKind::ScopeExit { .. }))
            .unwrap();
        for l in [3, 4] {
            b.instructions
                .insert(at, op(None, InstructionKind::StorageDead(LocalId(l))));
        }
    }
    for r in both(&m) {
        assert!(
            r.unwrap_err()
                .iter()
                .any(|d| d.message.contains("selected"))
        );
    }
}
#[test]
fn live_cleanup_is_once_and_storage_end_cannot_be_reopened_by_drop() {
    let mut m = module(true, false);
    let f = &mut m.functions[0];
    let at = f.blocks[2]
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::DropEnum { .. }))
        .unwrap();
    f.blocks[2].instructions.insert(
        at + 1,
        op(None, InstructionKind::DropEnum { local: LocalId(0) }),
    );
    assert!(
        analyze(&m, &Limits::default())
            .unwrap_err()
            .iter()
            .any(|d| d.message.contains("duplicate"))
    );
    let mut m = module(true, false);
    let f = &mut m.functions[0];
    let at = f.blocks[2]
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::ScopeExit { .. }))
        .unwrap();
    f.blocks[2].instructions.insert(
        at,
        op(None, InstructionKind::DropEnum { local: LocalId(0) }),
    );
    assert!(analyze(&m, &Limits::default()).is_err());
}
#[test]
fn mixed_same_type_leaves_keep_actual_created_position_under_target_permutation() {
    for swap in [false, true] {
        for selected in [0, 1] {
            let mut m = module(false, swap);
            let f = &mut m.functions[0];
            f.locals.push(LocalDecl {
                id: LocalId(3),
                name: "incoming".into(),
                ty: leaf(),
                mutable: true,
                span: Span::default(),
            });
            f.parameters.push(LocalId(3));
            let at = f.blocks[0]
                .instructions
                .iter()
                .position(|i| i.result == Some(ValueId(2)))
                .unwrap();
            f.blocks[0].instructions[at] = op(
                Some(ValueId(2)),
                InstructionKind::TakeLocal { local: LocalId(3) },
            );
            let targets = if swap {
                [LocalId(2), LocalId(1)]
            } else {
                [LocalId(1), LocalId(2)]
            };
            let chosen = targets[selected];
            let at = f.blocks[0]
                .instructions
                .iter()
                .position(|i| matches!(i.kind,InstructionKind::DropManaged{local} if local==chosen))
                .unwrap();
            let out = ValueId(f.values.len() as u32);
            f.values.push(leaf());
            f.blocks[0].instructions[at] =
                op(Some(out), InstructionKind::TakeLocal { local: chosen });
            let at = f.blocks[0]
                .instructions
                .iter()
                .position(|i| matches!(i.kind, InstructionKind::ScopeExit { .. }))
                .unwrap();
            f.blocks[0]
                .instructions
                .insert(at, op(None, InstructionKind::StorageDead(LocalId(3))));
            f.return_type = leaf();
            f.result_mode = PassingMode::Create;
            f.blocks[0].terminator = Terminator::Return(Some(out));
            for result in both(&m) {
                if selected == 0 {
                    result.unwrap();
                } else {
                    assert!(
                        result
                            .unwrap_err()
                            .iter()
                            .any(|d| d.message.contains("create result"))
                    );
                }
            }
        }
    }
}
#[test]
fn product_store_inside_sum_and_error_union_checks_actual_canonical_definition() {
    let element = Type::Package {
        name: "Element".into(),
        fields: vec![("token".into(), leaf())],
    };
    let store = Type::Managed(Box::new(Type::OwnedStore {
        element: Box::new(element.clone()),
    }));
    let root = Type::Package {
        name: "Root".into(),
        fields: vec![("items".into(), store)],
    };
    let mut m = module(true, false);
    m.resources.push(ResourceDefinition {
        name: "@builtin:OwnedStore<Element>".into(),
        payload: Type::OwnedStore {
            element: Box::new(element),
        },
        teardown: None,
    });
    let e = Type::Enum {
        name: "Storage".into(),
        variants: vec![
            ("Both".into(), vec![root.clone(), leaf()]),
            ("None".into(), vec![]),
        ],
    };
    m.functions[0].locals[0].ty = e;
    m.functions[0].locals[1].ty = root.clone();
    for b in &mut m.functions[0].blocks {
        for i in &mut b.instructions {
            if matches!(i.kind, InstructionKind::DropManaged { local: LocalId(1) }) {
                i.kind = InstructionKind::DropPackage { local: LocalId(1) };
            }
        }
    }
    for r in both(&m) {
        r.unwrap();
    }
    let mut union = m.clone();
    let ty = Type::ErrorUnion {
        error: Box::new(Type::I32),
        result: Box::new(root),
    };
    let f = &mut union.functions[0];
    f.locals.truncate(1);
    f.locals[0].ty = ty.clone();
    f.return_type = ty.clone();
    f.values = vec![ty];
    f.blocks = vec![BasicBlock {
        id: BlockId(0),
        instructions: vec![
            op(None, InstructionKind::ScopeEnter { scope: ScopeId(0) }),
            op(
                Some(ValueId(0)),
                InstructionKind::TakeLocal { local: LocalId(0) },
            ),
            op(None, InstructionKind::StorageDead(LocalId(0))),
            op(
                None,
                InstructionKind::ScopeExit {
                    scope: ScopeId(0),
                    kind: CleanupExitKind::Return,
                },
            ),
        ],
        terminator: Terminator::Return(Some(ValueId(0))),
    }];
    for r in both(&union) {
        r.unwrap();
    }
    for mut bad in [m, union] {
        bad.resources.last_mut().unwrap().name = "wrong canonical Store".into();
        assert!(analyze(&bad, &Limits::default()).is_err());
    }
}
#[test]
fn bypassing_predecessor_cannot_confer_a_guard_at_join() {
    let mut m = module(true, false);
    let f = &mut m.functions[0];
    // The false edge now reaches the same destructive arm. Joining possible tags
    // must not turn one predecessor's true edge into universal permission.
    if let Terminator::Branch { else_block, .. } = &mut f.blocks[0].terminator {
        *else_block = BlockId(1);
    }
    for r in both(&m) {
        assert!(r.is_err());
    }
}

#[test]
fn unpack_initialization_targets_are_part_of_incremental_semantic_key() {
    let m = module(false, false);
    let mut session = AnalysisSession::default();
    session.analyze(&m, &Limits::default()).unwrap();
    session.analyze(&m, &Limits::default()).unwrap();
    assert_eq!(session.last_stats().reused_functions, 1);
    let mut changed = m.clone();
    for i in &mut changed.functions[0].blocks[0].instructions {
        if let InstructionKind::UnpackVariant { fields, .. } = &mut i.kind {
            fields.swap(0, 1);
        }
    }
    // The target mapping is legal, but unchanged cleanup is now in the wrong
    // actual initialization order. Initialization itself must be recomputed.
    assert!(session.analyze(&changed, &Limits::default()).is_err());
    assert_eq!(session.last_stats().executed_functions, 1);
    assert!(analyze(&changed, &Limits::default()).is_err());
    session.analyze(&m, &Limits::default()).unwrap();
    assert_eq!(session.last_stats().reused_functions, 1);
}

#[test]
fn unpack_targets_interleave_with_mandatory_defer_by_actual_new_age() {
    let mut m = module(false, false);
    let f = &mut m.functions[0];
    f.locals.push(LocalDecl {
        id: LocalId(3),
        name: "cleanup".into(),
        ty: Type::Closure(ClosureId(0)),
        mutable: false,
        span: Span::default(),
    });
    f.blocks[0]
        .instructions
        .insert(1, op(None, InstructionKind::StorageLive(LocalId(3))));
    let action = ValueId(f.values.len() as u32);
    f.values.push(Type::Closure(ClosureId(0)));
    let taken = ValueId(f.values.len() as u32);
    f.values.push(Type::Closure(ClosureId(0)));
    let at = f.blocks[0]
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::UnpackVariant { .. }))
        .unwrap();
    f.blocks[0].instructions.splice(
        at..at,
        [
            op(
                Some(action),
                InstructionKind::MakeClosure {
                    closure: ClosureId(0),
                    captures: vec![],
                },
            ),
            op(
                None,
                InstructionKind::Store {
                    local: LocalId(3),
                    value: action,
                },
            ),
            op(
                None,
                InstructionKind::RegisterDefer {
                    scope: ScopeId(0),
                    local: LocalId(3),
                },
            ),
        ],
    );
    let at = f.blocks[0]
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::StorageDead(_)))
        .unwrap();
    f.blocks[0].instructions.splice(
        at..at,
        [
            op(
                Some(taken),
                InstructionKind::TakeLocal { local: LocalId(3) },
            ),
            op(
                None,
                InstructionKind::CallClosureOnce {
                    callee: taken,
                    arguments: vec![],
                },
            ),
            op(None, InstructionKind::StorageDead(LocalId(3))),
        ],
    );
    m.closures.push(ClosureDefinition {
        id: ClosureId(0),
        function: FunctionId(1),
        captures: vec![],
        parameters: vec![],
        result: Type::Unit,
        kind: CallableKind::FnOnce,
        lending_captures: vec![],
    });
    m.functions.push(TypedFunction {
        callable_return_contract: None,
        callable_contract: None,
        id: FunctionId(1),
        name: "cleanup_body".into(),
        foreign: None,
        environment: Some(ClosureId(0)),
        parameters: vec![],
        return_type: Type::Unit,
        result_mode: PassingMode::Infer,
        locals: vec![],
        values: vec![],
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![],
            terminator: Terminator::Return(None),
        }],
        entry: BlockId(0),
        span: Span::default(),
    });
    for r in both(&m) {
        r.unwrap();
    }
    let f = &mut m.functions[0];
    let call = f.blocks[0]
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::TakeLocal { local: LocalId(3) }))
        .unwrap();
    let pair: Vec<_> = f.blocks[0].instructions.drain(call..call + 2).collect();
    let first_drop = f.blocks[0]
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::DropManaged { .. }))
        .unwrap();
    f.blocks[0]
        .instructions
        .splice(first_drop..first_drop, pair);
    assert!(analyze(&m, &Limits::default()).is_err());
}

#[test]
fn ordinary_sum_admission_does_not_qualify_storage_or_borrowed_profiles() {
    let mut m = module(true, false);
    let element = Type::Package {
        name: "ForbiddenElement".into(),
        fields: vec![("sum".into(), sum())],
    };
    let store = Type::Managed(Box::new(Type::OwnedStore {
        element: Box::new(element.clone()),
    }));
    // The new shape is unused; no executable operation may hide its profile.
    m.functions[0].locals.push(LocalDecl {
        id: LocalId(3),
        name: "unused_store".into(),
        ty: store,
        mutable: false,
        span: Span::default(),
    });
    m.resources.push(ResourceDefinition {
        name: "@builtin:OwnedStore<ForbiddenElement>".into(),
        payload: Type::OwnedStore {
            element: Box::new(element),
        },
        teardown: None,
    });
    for result in both(&m) {
        assert!(
            result
                .unwrap_err()
                .iter()
                .any(|d| d.message.contains("qualification"))
        );
    }
    for ty in [
        Type::Array {
            element: Box::new(sum()),
            length: 1,
        },
        Type::Address {
            inner: Box::new(sum()),
            mutable: false,
        },
    ] {
        let mut m = module(true, false);
        m.functions[0].locals.push(LocalDecl {
            id: LocalId(3),
            name: "unused_profile".into(),
            ty,
            mutable: false,
            span: Span::default(),
        });
        for result in both(&m) {
            assert!(result.is_err());
        }
    }
}

#[test]
fn loop_reexecuting_the_same_store_requires_a_new_current_tag_observation() {
    for fresh in [false, true] {
        let mut m = module(true, false);
        let identity = TypedFunction {
            callable_return_contract: None,
            callable_contract: None,
            id: FunctionId(1),
            name: "relay".into(),
            foreign: None,
            environment: None,
            parameters: vec![LocalId(0)],
            return_type: sum(),
            result_mode: PassingMode::Infer,
            locals: vec![LocalDecl {
                id: LocalId(0),
                name: "input".into(),
                ty: sum(),
                mutable: false,
                span: Span::default(),
            }],
            values: vec![sum()],
            blocks: vec![BasicBlock {
                id: BlockId(0),
                instructions: vec![
                    op(None, InstructionKind::ScopeEnter { scope: ScopeId(0) }),
                    op(
                        Some(ValueId(0)),
                        InstructionKind::TakeLocal { local: LocalId(0) },
                    ),
                    op(None, InstructionKind::StorageDead(LocalId(0))),
                    op(
                        None,
                        InstructionKind::ScopeExit {
                            scope: ScopeId(0),
                            kind: CleanupExitKind::Return,
                        },
                    ),
                ],
                terminator: Terminator::Return(Some(ValueId(0))),
            }],
            entry: BlockId(0),
            span: Span::default(),
        };
        let f = &mut m.functions[0];
        let at = f.blocks[0]
            .instructions
            .iter()
            .position(|i| matches!(i.kind, InstructionKind::VariantTagLocal { .. }))
            .unwrap();
        let mut observation = f.blocks[0].instructions.split_off(at);
        let tag = observation.remove(0);
        let moved = ValueId(f.values.len() as u32);
        f.values.push(sum());
        let returned = ValueId(f.values.len() as u32);
        f.values.push(sum());
        let mut instructions = vec![
            op(
                Some(moved),
                InstructionKind::TakeLocal { local: LocalId(0) },
            ),
            op(
                Some(returned),
                InstructionKind::Call {
                    function: FunctionId(1),
                    arguments: vec![moved],
                },
            ),
            op(
                None,
                InstructionKind::Store {
                    local: LocalId(0),
                    value: returned,
                },
            ),
        ];
        instructions.insert(if fresh { 3 } else { 0 }, tag);
        instructions.extend(observation);
        f.blocks[0].terminator = Terminator::Jump(BlockId(3));
        f.blocks.push(BasicBlock {
            id: BlockId(3),
            instructions,
            terminator: Terminator::Branch {
                condition: ValueId(2),
                then_block: BlockId(1),
                else_block: BlockId(3),
            },
        });
        m.functions.push(identity);
        for r in both(&m) {
            if fresh {
                r.unwrap();
            } else {
                assert!(
                    r.unwrap_err()
                        .iter()
                        .any(|d| d.message.contains("selected"))
                );
            }
        }
    }
}

#[test]
fn malformed_or_unexecuted_ssa_cannot_supply_tag_authority() {
    for mode in 0..3 {
        let mut m = module(true, false);
        let f = &mut m.functions[0];
        match mode {
            0 => {
                let at = f.blocks[0]
                    .instructions
                    .iter()
                    .position(|i| matches!(i.kind, InstructionKind::Binary { .. }))
                    .unwrap();
                f.blocks[0].instructions.insert(
                    at,
                    op(
                        Some(ValueId(0)),
                        InstructionKind::Constant(Constant::I32(0)),
                    ),
                );
            }
            1 => {
                f.values[1] = Type::Bool;
            }
            _ => {
                let at = f.blocks[0]
                    .instructions
                    .iter()
                    .position(|i| matches!(i.kind, InstructionKind::Constant(_)))
                    .unwrap();
                let constant = f.blocks[0].instructions.remove(at);
                f.blocks.push(BasicBlock {
                    id: BlockId(3),
                    instructions: vec![constant],
                    terminator: Terminator::Trap,
                });
            }
        }
        for r in both(&m) {
            assert!(r.is_err());
        }
    }
    let mut m = module(true, false);
    m.functions[0].locals[0].id = LocalId(99);
    assert!(analyze(&m, &Limits::default()).is_err());
}

#[test]
fn ordinary_value_calls_do_not_export_private_created_leaf_freshness() {
    for identity in [true, false] {
        let mut m = module(false, false);
        let mut callee = m.functions[0].clone();
        callee.id = FunctionId(1);
        callee.name = if identity { "relay" } else { "fresh_factory" }.into();
        callee.return_type = sum();
        callee.result_mode = PassingMode::Infer;
        if identity {
            callee.locals.truncate(1);
            callee.parameters = vec![LocalId(0)];
            callee.values = vec![sum()];
            callee.blocks = vec![BasicBlock {
                id: BlockId(0),
                instructions: vec![
                    op(None, InstructionKind::ScopeEnter { scope: ScopeId(0) }),
                    op(
                        Some(ValueId(0)),
                        InstructionKind::TakeLocal { local: LocalId(0) },
                    ),
                    op(None, InstructionKind::StorageDead(LocalId(0))),
                    op(
                        None,
                        InstructionKind::ScopeExit {
                            scope: ScopeId(0),
                            kind: CleanupExitKind::Return,
                        },
                    ),
                ],
                terminator: Terminator::Return(Some(ValueId(0))),
            }];
        } else {
            let at = callee.blocks[0]
                .instructions
                .iter()
                .position(|i| {
                    matches!(
                        i.kind,
                        InstructionKind::Store {
                            local: LocalId(0),
                            ..
                        }
                    )
                })
                .unwrap();
            callee.blocks[0].instructions.truncate(at);
            for l in [2, 1, 0] {
                callee.blocks[0]
                    .instructions
                    .push(op(None, InstructionKind::StorageDead(LocalId(l))));
            }
            callee.blocks[0].instructions.push(op(
                None,
                InstructionKind::ScopeExit {
                    scope: ScopeId(0),
                    kind: CleanupExitKind::Return,
                },
            ));
            callee.blocks[0].terminator = Terminator::Return(Some(ValueId(3)));
        }
        let f = &mut m.functions[0];
        if identity {
            let at = f.blocks[0]
                .instructions
                .iter()
                .position(|i| matches!(i.kind, InstructionKind::UnpackVariant { .. }))
                .unwrap();
            let argument = ValueId(f.values.len() as u32);
            f.values.push(sum());
            let returned = ValueId(f.values.len() as u32);
            f.values.push(sum());
            f.blocks[0].instructions.splice(
                at..at,
                [
                    op(
                        Some(argument),
                        InstructionKind::TakeLocal { local: LocalId(0) },
                    ),
                    op(
                        Some(returned),
                        InstructionKind::Call {
                            function: FunctionId(1),
                            arguments: vec![argument],
                        },
                    ),
                    op(
                        None,
                        InstructionKind::Store {
                            local: LocalId(0),
                            value: returned,
                        },
                    ),
                ],
            );
        } else {
            f.blocks[0].instructions.retain(|i| {
                !matches!(
                    i.kind,
                    InstructionKind::Constant(_)
                        | InstructionKind::MakeManaged { .. }
                        | InstructionKind::MakeVariant { .. }
                )
            });
            let at = f.blocks[0]
                .instructions
                .iter()
                .position(|i| {
                    matches!(
                        i.kind,
                        InstructionKind::Store {
                            local: LocalId(0),
                            ..
                        }
                    )
                })
                .unwrap();
            f.blocks[0].instructions.insert(
                at,
                op(
                    Some(ValueId(3)),
                    InstructionKind::Call {
                        function: FunctionId(1),
                        arguments: vec![],
                    },
                ),
            );
        }
        // A real success edge observes the returned carrier's actual tag. The
        // known construction from before relay is deliberately not authority.
        let at = f.blocks[0]
            .instructions
            .iter()
            .position(|i| matches!(i.kind, InstructionKind::UnpackVariant { .. }))
            .unwrap();
        let tail = f.blocks[0].instructions.split_off(at);
        let tag = ValueId(f.values.len() as u32);
        f.values.push(Type::I32);
        let c = ValueId(f.values.len() as u32);
        f.values.push(Type::I32);
        let condition = ValueId(f.values.len() as u32);
        f.values.push(Type::Bool);
        f.blocks[0].instructions.extend([
            op(
                Some(tag),
                InstructionKind::VariantTagLocal {
                    local: LocalId(0),
                    path: vec![],
                },
            ),
            op(Some(c), InstructionKind::Constant(Constant::I32(0))),
            op(
                Some(condition),
                InstructionKind::Binary {
                    op: BinaryOp::Equal,
                    left: tag,
                    right: c,
                },
            ),
        ]);
        f.blocks[0].terminator = Terminator::Branch {
            condition,
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
        let at = f.blocks[1]
            .instructions
            .iter()
            .position(|i| matches!(i.kind, InstructionKind::DropManaged { local: LocalId(1) }))
            .unwrap();
        let out = ValueId(f.values.len() as u32);
        f.values.push(leaf());
        f.blocks[1].instructions[at] =
            op(Some(out), InstructionKind::TakeLocal { local: LocalId(1) });
        f.return_type = leaf();
        f.result_mode = PassingMode::Create;
        f.blocks[1].terminator = Terminator::Return(Some(out));
        m.functions.push(callee);
        for r in both(&m) {
            assert!(
                r.unwrap_err()
                    .iter()
                    .any(|d| d.message.contains("create result"))
            );
        }
    }
}
