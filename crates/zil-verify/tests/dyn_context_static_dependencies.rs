//! Real fat-slice getter inventory, with an independently written raw callback
//! cycle. The original trait, provider, slot and Self class maps stay intact.
use zil_base::Limits;
use zil_ir::*;

fn getter() -> (TypedModule, CandidatePlan) {
    wire::decode_module_candidate(
        include_bytes!("fixtures/dyn-context/context-long-slice-getter.candidate.wire"),
        &Limits::default(),
    )
    .unwrap()
}
fn checked(module: &TypedModule, plan: &CandidatePlan) {
    let limits = Limits::default();
    zil_verify::verify_module(module.clone(), plan.clone(), &module.target, &limits).unwrap();
    let bytes = wire::encode_module_candidate(module, plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&bytes, &module.target, &limits).unwrap();
}
#[test]
fn real_original_slice_getter_and_context_callback_sources_are_verified() {
    let (module, plan) = getter();
    assert!(module.dyn_interfaces.iter().any(|interface| {
        interface.slots.iter().any(|slot| {
            slot.result
                == Type::Address {
                    mutable: false,
                    inner: Box::new(Type::Slice {
                        element: Box::new(Type::U8),
                    }),
                }
                && slot
                    .storage_life
                    .as_ref()
                    .unwrap()
                    .bindings
                    .iter()
                    .any(|row| {
                        row.position.base == StorageBase::Result
                            && row.binder == StorageBinder::Fixed(0)
                    })
        })
    }));
    checked(&module, &plan);
    let (module, plan) = wire::decode_module_candidate(
        include_bytes!("fixtures/dyn-context/context-captured-owner.candidate.wire"),
        &Limits::default(),
    )
    .unwrap();
    checked(&module, &plan);
}

fn blank(template: &TypedFunction, id: FunctionId, name: &str, result: Type) -> TypedFunction {
    let mut f = template.clone();
    f.id = id;
    f.name = name.into();
    f.environment = None;
    f.dyn_instance = None;
    f.foreign = None;
    f.parameters.clear();
    f.locals.clear();
    f.values.clear();
    f.blocks.clear();
    f.entry = BlockId(0);
    f.return_type = result;
    f.result_mode = PassingMode::Infer;
    f.call_kind = CallKind::Direct;
    f.resume_bound = TaskResumeBound::Local;
    f.storage_life = None;
    f.reference_return_bound = None;
    f.retained_return_sources = None;
    f.callable_contract = None;
    f.callable_return_contract = None;
    f.protocol_contracts.clear();
    f
}
fn empty_life() -> StorageLifeContract {
    StorageLifeContract {
        fixed_binder_count: 0,
        invocation_binder_count: 0,
        bindings: vec![],
        static_bindings: vec![],
        classes: vec![],
    }
}
fn position(base: StorageBase) -> StoragePosition {
    StoragePosition { base, path: vec![] }
}
fn instruction(f: &TypedFunction, result: Option<u32>, kind: InstructionKind) -> Instruction {
    Instruction {
        result: result.map(ValueId),
        kind,
        span: f.span,
    }
}
fn plan(
    function: &TypedFunction,
    sets: &[(Vec<u32>, Vec<u32>)],
    holds: Vec<Vec<HoldRoot>>,
) -> FunctionPlan {
    assert_eq!(function.blocks.len(), sets.len());
    assert_eq!(function.values.len(), holds.len());
    FunctionPlan {
        function: function.id,
        blocks: sets
            .iter()
            .enumerate()
            .map(|(block, (input, output))| BlockPlan {
                block: BlockId(block as u32),
                initialized_in: input.iter().copied().map(LocalId).collect(),
                initialized_out: output.iter().copied().map(LocalId).collect(),
            })
            .collect(),
        holds: holds
            .into_iter()
            .enumerate()
            .map(|(value, roots)| ValueHold {
                value: ValueId(value as u32),
                roots,
            })
            .collect(),
    }
}

/// The raw control adds a constant-slice callback factory. Its literal branch
/// is the sole *actual* seed of that callback field. The other branch calls the
/// unchanged Text<'a>.text slot through create("unused"). create's public upper
/// includes this unused literal argument, while its actual data comes from the
/// callback. Removing only the literal CFG edge leaves a conditional cycle.
fn callback_cycle(literal_edge: bool) -> (TypedModule, CandidatePlan) {
    let (mut module, mut candidate) = getter();
    assert!(module.closures.is_empty());
    let create = module
        .functions
        .iter()
        .position(|f| f.name == "create")
        .unwrap();
    let Type::ErrorUnion { result, .. } = &module.functions[create].return_type else {
        panic!()
    };
    let owner = result.as_ref().clone();
    let Type::OwnedDyn { interface } = owner else {
        panic!()
    };
    let slice = module
        .dyn_interfaces
        .iter()
        .find(|i| i.id == interface)
        .unwrap()
        .slots[0]
        .result
        .clone();
    let error = module.functions[create].return_type.clone();
    let template = module.functions[create].clone();
    let entry = FunctionId(module.functions.len() as u32);
    let make = FunctionId(entry.0 + 1);
    let closure = ClosureId(0);
    let callback = Type::Closure(closure);
    let mut scope = empty_life();
    scope.static_bindings = vec![
        position(StorageBase::Capture(0)),
        position(StorageBase::Result),
    ];
    module.closures.push(ClosureDefinition {
        id: closure,
        function: entry,
        captures: vec![slice.clone()],
        parameters: vec![],
        result: slice.clone(),
        kind: CallableKind::Fn,
        lending_captures: vec![],
        storage_life: Some(scope.clone()),
    });
    let mut body = blank(&template, entry, "$raw_constant_text_entry", slice.clone());
    body.environment = Some(closure);
    body.storage_life = Some(scope);
    body.retained_return_sources = Some(vec![StorageSourceBinding {
        position: position(StorageBase::Result),
        sources: vec![StorageSource::Static {
            leaf: slice.clone(),
            mode: StoredBorrowMode::SharedAddress,
        }],
    }]);
    body.values = vec![slice.clone()];
    body.blocks = vec![BasicBlock {
        id: BlockId(0),
        instructions: vec![
            instruction(
                &body,
                None,
                InstructionKind::ScopeEnter { scope: ScopeId(0) },
            ),
            instruction(&body, Some(0), InstructionKind::ReadCapture { index: 0 }),
            instruction(
                &body,
                None,
                InstructionKind::ScopeExit {
                    scope: ScopeId(0),
                    kind: CleanupExitKind::Return,
                },
            ),
        ],
        terminator: Terminator::Return(Some(ValueId(0))),
    }];
    candidate.functions.push(plan(
        &body,
        &[(vec![], vec![])],
        vec![vec![HoldRoot::Static]],
    ));
    module.functions.push(body);

    let mut factory = blank(&template, make, "$raw_make_constant_text", callback.clone());
    factory.storage_life = Some(empty_life());
    let mut public = empty_life();
    public.static_bindings.push(position(StorageBase::Result));
    factory.callable_return_contract = Some(CallableReturnContract {
        closure,
        kind: CallableKind::Fn,
        source: CallableReturnSource::Fresh,
        life: Some(public),
        hold: vec![],
        call: vec![],
        writes: vec![],
        cleanup: vec![],
        lending_captures: vec![],
        capture_sources: Some(vec![StorageSourceBinding {
            position: position(StorageBase::Capture(0)),
            sources: vec![StorageSource::Static {
                leaf: slice.clone(),
                mode: StoredBorrowMode::SharedAddress,
            }],
        }]),
    });
    factory.parameters = vec![LocalId(0)];
    for (index, (name, ty, mutable)) in [
        ("literal_branch", Type::Bool, false),
        ("text", slice.clone(), true),
        ("outcome", error.clone(), false),
        ("owner", owner.clone(), false),
    ]
    .into_iter()
    .enumerate()
    {
        factory.locals.push(LocalDecl {
            id: LocalId(index as u32),
            name: name.into(),
            ty,
            mutable,
            span: factory.span,
        });
    }
    factory.values = vec![
        Type::Bool,
        slice.clone(),
        slice.clone(),
        error,
        Type::I32,
        Type::I32,
        Type::Bool,
        owner,
        Type::Address {
            mutable: false,
            inner: Box::new(Type::Dyn { interface }),
        },
        slice.clone(),
        slice.clone(),
        callback.clone(),
    ];
    let inst = |result, kind| instruction(&factory, result, kind);
    factory.blocks = vec![
        BasicBlock {
            id: BlockId(0),
            instructions: vec![
                inst(None, InstructionKind::ScopeEnter { scope: ScopeId(0) }),
                inst(Some(0), InstructionKind::Load(LocalId(0))),
            ],
            terminator: Terminator::Branch {
                condition: ValueId(0),
                then_block: BlockId(if literal_edge { 1 } else { 2 }),
                else_block: BlockId(2),
            },
        },
        BasicBlock {
            id: BlockId(1),
            instructions: vec![
                inst(
                    Some(1),
                    InstructionKind::StaticUtf8 {
                        bytes: b"abc".to_vec(),
                    },
                ),
                inst(
                    None,
                    InstructionKind::Store {
                        local: LocalId(1),
                        value: ValueId(1),
                    },
                ),
            ],
            terminator: Terminator::Jump(BlockId(5)),
        },
        BasicBlock {
            id: BlockId(2),
            instructions: vec![
                inst(
                    Some(2),
                    InstructionKind::StaticUtf8 {
                        bytes: b"unused".to_vec(),
                    },
                ),
                inst(
                    Some(3),
                    InstructionKind::Call {
                        function: template.id,
                        arguments: vec![ValueId(2)],
                    },
                ),
                inst(
                    None,
                    InstructionKind::Store {
                        local: LocalId(2),
                        value: ValueId(3),
                    },
                ),
                inst(
                    Some(4),
                    InstructionKind::PlaceTagLocal {
                        local: LocalId(2),
                        path: vec![],
                    },
                ),
                inst(Some(5), InstructionKind::Constant(Constant::I32(1))),
                inst(
                    Some(6),
                    InstructionKind::Binary {
                        op: BinaryOp::Equal,
                        left: ValueId(4),
                        right: ValueId(5),
                    },
                ),
            ],
            terminator: Terminator::Branch {
                condition: ValueId(6),
                then_block: BlockId(3),
                else_block: BlockId(4),
            },
        },
        BasicBlock {
            id: BlockId(3),
            instructions: vec![
                inst(None, InstructionKind::DropErrorUnion { local: LocalId(2) }),
                inst(
                    None,
                    InstructionKind::ScopeExit {
                        scope: ScopeId(0),
                        kind: CleanupExitKind::Error,
                    },
                ),
            ],
            terminator: Terminator::Trap,
        },
        BasicBlock {
            id: BlockId(4),
            instructions: vec![
                inst(
                    Some(7),
                    InstructionKind::TakeVariantField {
                        local: LocalId(2),
                        path: vec![PlaceProjection::ErrorArm(false)],
                    },
                ),
                inst(None, InstructionKind::DropErrorUnion { local: LocalId(2) }),
                inst(
                    None,
                    InstructionKind::Store {
                        local: LocalId(3),
                        value: ValueId(7),
                    },
                ),
                inst(
                    Some(8),
                    InstructionKind::BorrowDynOwned {
                        local: LocalId(3),
                        mutable: false,
                    },
                ),
                inst(
                    Some(9),
                    InstructionKind::CallDynSlot {
                        callee: ValueId(8),
                        interface,
                        slot: 0,
                        arguments: vec![],
                    },
                ),
                inst(
                    None,
                    InstructionKind::Store {
                        local: LocalId(1),
                        value: ValueId(9),
                    },
                ),
                inst(None, InstructionKind::DropDynOwned { local: LocalId(3) }),
            ],
            terminator: Terminator::Jump(BlockId(5)),
        },
        BasicBlock {
            id: BlockId(5),
            instructions: vec![
                inst(Some(10), InstructionKind::Load(LocalId(1))),
                inst(
                    Some(11),
                    InstructionKind::MakeClosure {
                        closure,
                        captures: vec![ValueId(10)],
                    },
                ),
                inst(
                    None,
                    InstructionKind::ScopeExit {
                        scope: ScopeId(0),
                        kind: CleanupExitKind::Return,
                    },
                ),
            ],
            terminator: Terminator::Return(Some(ValueId(11))),
        },
    ];
    candidate.functions.push(plan(
        &factory,
        &[
            (vec![0], vec![0]),
            (
                if literal_edge { vec![0] } else { vec![] },
                if literal_edge { vec![0, 1] } else { vec![1] },
            ),
            (vec![0], vec![0, 2]),
            (vec![0, 2], vec![0]),
            (vec![0, 2], vec![0, 1]),
            (vec![0, 1], vec![0, 1]),
        ],
        vec![
            vec![],
            vec![HoldRoot::Static],
            vec![HoldRoot::Static],
            vec![HoldRoot::Static],
            vec![],
            vec![],
            vec![],
            vec![HoldRoot::Static],
            vec![HoldRoot::Local(LocalId(3)), HoldRoot::Static],
            vec![HoldRoot::Static],
            vec![HoldRoot::Static],
            vec![HoldRoot::Static],
        ],
    ));
    module.functions.push(factory);

    let f = &mut module.functions[create];
    let selected = f
        .blocks
        .iter()
        .enumerate()
        .find_map(|(block, body)| {
            body.instructions.iter().enumerate().find_map(|(index, i)| {
                matches!(i.kind, InstructionKind::Load(local) if local == f.parameters[0])
                    .then_some((block, index, i.result.unwrap()))
            })
        })
        .unwrap();
    let callback_local = LocalId(f.locals.len() as u32);
    f.locals.push(LocalDecl {
        id: callback_local,
        name: "$raw_callback".into(),
        ty: callback.clone(),
        mutable: false,
        span: f.span,
    });
    let flag = ValueId(f.values.len() as u32);
    let value = ValueId(flag.0 + 1);
    f.values.extend([Type::Bool, callback]);
    let replacement = vec![
        instruction(
            f,
            Some(flag.0),
            InstructionKind::Constant(Constant::Bool(true)),
        ),
        instruction(
            f,
            Some(value.0),
            InstructionKind::Call {
                function: make,
                arguments: vec![flag],
            },
        ),
        instruction(
            f,
            None,
            InstructionKind::Store {
                local: callback_local,
                value,
            },
        ),
        instruction(
            f,
            Some(selected.2.0),
            InstructionKind::CallClosureShared {
                callee: callback_local,
                arguments: vec![],
            },
        ),
        instruction(
            f,
            None,
            InstructionKind::DropClosure {
                local: callback_local,
            },
        ),
    ];
    f.blocks[selected.0]
        .instructions
        .splice(selected.1..=selected.1, replacement);
    f.retained_return_sources.as_mut().unwrap()[0]
        .sources
        .push(StorageSource::Static {
            leaf: slice,
            mode: StoredBorrowMode::SharedAddress,
        });
    for hold in &mut candidate.functions[create].holds {
        for root in &mut hold.roots {
            if *root == HoldRoot::Parameter(0) {
                *root = HoldRoot::Static;
            }
        }
        hold.roots.sort_unstable();
        hold.roots.dedup();
    }
    candidate.functions[create].holds.extend([
        ValueHold {
            value: flag,
            roots: vec![],
        },
        ValueHold {
            value,
            roots: vec![HoldRoot::Static],
        },
    ]);
    candidate.module_fingerprint = module.fingerprint();
    (module, candidate)
}

#[test]
fn context_canonical_upper_cannot_seed_a_callback_cycle_from_an_unused_literal() {
    let (module, plan) = callback_cycle(true);
    let directory = std::env::var_os("ZIL_TEST_REPORT_DIR").map(|root| {
        let directory = std::path::PathBuf::from(root)
            .join(format!("context-static-cycle-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(
            directory.join("control-unverified.wire"),
            wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap(),
        )
        .unwrap();
        std::fs::write(
            directory.join("control.txt"),
            format!("{module:#?}\n{plan:#?}"),
        )
        .unwrap();
        directory
    });
    checked(&module, &plan);
    if let Some(directory) = &directory {
        std::fs::write(
            directory.join("verified-control.wire"),
            wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap(),
        )
        .unwrap();
    }
    let (module, plan) = callback_cycle(false);
    let limits = Limits::default();
    if let Some(directory) = &directory {
        std::fs::write(
            directory.join("cycle-unverified.wire"),
            wire::encode_module_candidate(&module, &plan, &limits).unwrap(),
        )
        .unwrap();
        std::fs::write(
            directory.join("cycle.txt"),
            format!("{module:#?}\n{plan:#?}"),
        )
        .unwrap();
    }
    for wire in [false, true] {
        let errors = if wire {
            let bytes = wire::encode_module_candidate(&module, &plan, &limits).unwrap();
            zil_verify::verify_module_bytes(&bytes, &module.target, &limits).unwrap_err()
        } else {
            zil_verify::verify_module(module.clone(), plan.clone(), &module.target, &limits)
                .unwrap_err()
        };
        assert!(
            errors.iter().any(|error| error.code == "E-VERIFY-STORED"
                && error
                    .message
                    .contains("dependency cycle has no actual program-static root")),
            "wire={wire}: {errors:?}"
        );
    }
}
