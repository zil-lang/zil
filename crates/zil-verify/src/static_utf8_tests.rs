//! Caller-built IR trust-boundary fixtures; no source/native evidence.
use super::*;
use zil_base::{FileId, Fingerprint, ModuleId, SCHEMA_VERSION, Span, TargetProfile};
fn span() -> Span {
    Span::new(FileId(0), 0, 1)
}
fn address() -> Type {
    Type::Address {
        mutable: false,
        inner: Box::new(Type::Slice {
            element: Box::new(Type::U8),
        }),
    }
}
fn inst(result: Option<u32>, kind: InstructionKind) -> Instruction {
    Instruction {
        result: result.map(ValueId),
        kind,
        span: span(),
    }
}
fn module() -> TypedModule {
    TypedModule {
        schema_version: SCHEMA_VERSION,
        module_id: ModuleId(0),
        source_fingerprint: Fingerprint::ZERO,
        target: TargetProfile::host_llvm(),
        resources: vec![],
        closures: vec![],
        unsupported: vec![],
        life_declarations: vec![],
        life_instances: vec![],
        dyn_declarations: vec![],
        dyn_interfaces: vec![],
        dyn_vtables: vec![],
        functions: vec![TypedFunction {
            storage_life: None,
            call_kind: CallKind::Direct,
            reference_return_bound: None,
            callable_return_contract: None,
            callable_contract: None,
            protocol_contract: None,
            result_mode: PassingMode::Infer,
            foreign: None,
            environment: None,
            id: FunctionId(0),
            name: "literal".into(),
            parameters: vec![],
            return_type: address(),
            locals: vec![].into(),
            values: vec![address()].into(),
            entry: BlockId(0),
            span: span(),
            blocks: vec![BasicBlock {
                id: BlockId(0),
                instructions: vec![inst(
                    Some(0),
                    InstructionKind::StaticUtf8 {
                        bytes: "a\0é🙂".as_bytes().to_vec(),
                    },
                )],
                terminator: Terminator::Return(Some(ValueId(0))),
            }]
            .into(),
        }],
    }
}
fn stored_module() -> TypedModule {
    let mut m = module();
    let mut probe = m.functions[0].clone();
    probe.id = FunctionId(1);
    probe.name = "identity".into();
    probe.locals = vec![LocalDecl {
        id: LocalId(0),
        name: "input".into(),
        ty: address(),
        mutable: false,
        span: span(),
    }]
    .into();
    probe.parameters = vec![LocalId(0)];
    probe.blocks[0].instructions = vec![inst(Some(0), InstructionKind::Load(LocalId(0)))];
    probe.storage_life = Some(StorageLifeContract {
        fixed_binder_count: 0,
        invocation_binder_count: 1,
        classes: vec![],
        bindings: vec![
            StorageLifeBinding {
                position: StoragePosition {
                    base: StorageBase::Parameter(0),
                    path: vec![],
                },
                binder: StorageBinder::Invocation(0),
            },
            StorageLifeBinding {
                position: StoragePosition {
                    base: StorageBase::Result,
                    path: vec![],
                },
                binder: StorageBinder::Invocation(0),
            },
        ],
    });
    m.functions.push(probe);
    m
}
fn helper_module() -> TypedModule {
    let mut m = module();
    let mut f = m.functions[0].clone();
    f.id = FunctionId(1);
    f.name = "caller".into();
    f.locals = vec![LocalDecl {
        id: LocalId(0),
        name: "slot".into(),
        ty: address(),
        mutable: false,
        span: span(),
    }]
    .into();
    f.values = vec![address(), address(), address()].into();
    f.blocks[0].instructions = vec![
        inst(
            Some(0),
            InstructionKind::Call {
                function: FunctionId(0),
                arguments: vec![],
            },
        ),
        inst(None, InstructionKind::StorageLive(LocalId(0))),
        inst(
            None,
            InstructionKind::Store {
                local: LocalId(0),
                value: ValueId(0),
            },
        ),
        inst(Some(1), InstructionKind::Load(LocalId(0))),
        inst(None, InstructionKind::StorageDead(LocalId(0))),
        inst(
            Some(2),
            InstructionKind::ReborrowAddress {
                address: ValueId(1),
                mutable: false,
            },
        ),
    ];
    f.blocks[0].terminator = Terminator::Return(Some(ValueId(2)));
    m.functions.push(f);
    m
}

fn candidate(m: &TypedModule) -> CandidatePlan {
    let mut c = CandidatePlan::empty_for(m);
    for f in &m.functions {
        c.functions.push(FunctionPlan {
            function: f.id,
            blocks: vec![BlockPlan {
                block: BlockId(0),
                initialized_in: f.parameters.clone(),
                initialized_out: f.parameters.clone(),
            }],
            holds: f
                .values
                .iter()
                .enumerate()
                .map(|(i, _)| ValueHold {
                    value: ValueId(i as u32),
                    roots: vec![if f.name == "identity" {
                        HoldRoot::Parameter(0)
                    } else {
                        HoldRoot::Static
                    }],
                })
                .collect(),
        });
    }
    c
}
fn check(m: TypedModule, c: CandidatePlan, limits: &Limits) -> CompileResult<VerifiedModule> {
    let target = m.target.clone();
    verify_module(m, c, &target, limits)
}
#[test]
fn static_utf8_verifies_static_helper_returns_and_pointer_slot_end_independently() {
    let m = helper_module();
    let c = candidate(&m);
    check(m, c, &Limits::default()).unwrap();
}
#[test]
fn static_utf8_verifies_actual_static_sources_in_stored_modules() {
    let m = stored_module();
    let c = candidate(&m);
    check(m, c, &Limits::default()).unwrap();
}
#[test]
fn static_utf8_rejects_omitted_or_forged_candidate_roots() {
    let m = module();
    for roots in [
        vec![],
        vec![HoldRoot::Parameter(0)],
        vec![HoldRoot::Local(LocalId(0))],
    ] {
        let mut c = candidate(&m);
        c.functions[0].holds[0].roots = roots;
        assert!(check(m.clone(), c, &Limits::default()).is_err());
    }
    let mut m = module();
    m.functions[0].return_type = Type::U8;
    m.functions[0].values[0] = Type::U8;
    m.functions[0].blocks[0].instructions[0].kind = InstructionKind::Constant(Constant::U8(0));
    let c = candidate(&m);
    assert!(check(m, c, &Limits::default()).is_err());
}
#[test]
fn static_utf8_validates_raw_encoding_and_precise_result_type() {
    for bytes in [
        vec![0xff],
        vec![0xc0, 0x80],
        vec![0xed, 0xa0, 0x80],
        vec![0xf4, 0x90, 0x80, 0x80],
        vec![0xe2, 0x82],
    ] {
        let mut m = module();
        m.functions[0].blocks[0].instructions[0].kind = InstructionKind::StaticUtf8 { bytes };
        let c = candidate(&m);
        assert_eq!(
            check(m, c, &Limits::default()).unwrap_err()[0].code,
            "E-VERIFY-UTF8"
        );
    }
    for ty in [
        Type::U8,
        Type::Address {
            mutable: true,
            inner: Box::new(Type::Slice {
                element: Box::new(Type::U8),
            }),
        },
        Type::Address {
            mutable: false,
            inner: Box::new(Type::Slice {
                element: Box::new(Type::U16),
            }),
        },
    ] {
        let mut m = module();
        m.functions[0].values[0] = ty;
        let c = candidate(&m);
        assert!(check(m, c, &Limits::default()).is_err());
    }
    let mut m = module();
    m.functions[0].blocks[0].instructions[0].result = None;
    let c = candidate(&m);
    assert!(check(m, c, &Limits::default()).is_err());
}
#[test]
fn static_utf8_rejects_mutable_upgrade_without_write() {
    for mut m in [module(), stored_module()] {
        let f = &mut m.functions[0];
        f.values.push(Type::Address {
            mutable: true,
            inner: Box::new(Type::Slice {
                element: Box::new(Type::U8),
            }),
        });
        f.blocks[0].instructions.push(inst(
            Some(1),
            InstructionKind::ReborrowAddress {
                address: ValueId(0),
                mutable: true,
            },
        ));
        let c = candidate(&m);
        assert!(check(m, c, &Limits::default()).is_err());
    }
}
#[test]
fn static_utf8_byte_limit_and_work_limit_reject_before_fingerprint() {
    let m = module();
    let c = candidate(&m);
    let l = Limits {
        max_nodes: 2,
        ..Limits::default()
    };
    assert_eq!(
        check(m.clone(), c.clone(), &l).unwrap_err()[0].category,
        zil_base::ResultCategory::ResourceLimit
    );
    let l = Limits {
        max_analysis_steps: 1,
        ..Limits::default()
    };
    assert_eq!(
        check(m.clone(), c.clone(), &l).unwrap_err()[0].category,
        zil_base::ResultCategory::ResourceLimit
    );
    check(m.clone(), c.clone(), &Limits::default()).unwrap();
    assert!(check(m, c, &l).is_err());
}

fn bounded_module(stored: bool) -> TypedModule {
    let mut m = if stored { stored_module() } else { module() };
    let f = &mut m.functions[0];
    f.locals = vec![LocalDecl {
        id: LocalId(0),
        name: "input".into(),
        ty: address(),
        mutable: false,
        span: span(),
    }]
    .into();
    f.parameters = vec![LocalId(0)];
    f.reference_return_bound = Some(vec![0]);
    if stored {
        f.reference_return_bound = None;
        f.storage_life = Some(StorageLifeContract {
            fixed_binder_count: 0,
            invocation_binder_count: 1,
            classes: vec![],
            bindings: vec![
                StorageLifeBinding {
                    position: StoragePosition {
                        base: StorageBase::Parameter(0),
                        path: vec![],
                    },
                    binder: StorageBinder::Invocation(0),
                },
                StorageLifeBinding {
                    position: StoragePosition {
                        base: StorageBase::Result,
                        path: vec![],
                    },
                    binder: StorageBinder::Invocation(0),
                },
            ],
        });
    }
    m
}
fn mixed_module() -> TypedModule {
    let mut m = bounded_module(false);
    let f = &mut m.functions[0];
    f.locals.push(LocalDecl {
        id: LocalId(1),
        name: "condition".into(),
        ty: Type::Bool,
        mutable: false,
        span: span(),
    });
    f.parameters.push(LocalId(1));
    f.values.extend([Type::Bool, address()]);
    f.blocks[0]
        .instructions
        .push(inst(Some(1), InstructionKind::Load(LocalId(1))));
    f.blocks[0].terminator = Terminator::Branch {
        condition: ValueId(1),
        then_block: BlockId(1),
        else_block: BlockId(2),
    };
    f.blocks.push(BasicBlock {
        id: BlockId(1),
        instructions: vec![],
        terminator: Terminator::Return(Some(ValueId(0))),
    });
    f.blocks.push(BasicBlock {
        id: BlockId(2),
        instructions: vec![inst(Some(2), InstructionKind::Load(LocalId(0)))],
        terminator: Terminator::Return(Some(ValueId(2))),
    });
    let mut caller = f.clone();
    caller.id = FunctionId(1);
    caller.name = "mixed_caller".into();
    caller.values = vec![address(), Type::Bool, address()].into();
    caller.blocks = vec![BasicBlock {
        id: BlockId(0),
        instructions: vec![
            inst(Some(0), InstructionKind::Load(LocalId(0))),
            inst(Some(1), InstructionKind::Load(LocalId(1))),
            inst(
                Some(2),
                InstructionKind::Call {
                    function: FunctionId(0),
                    arguments: vec![ValueId(0), ValueId(1)],
                },
            ),
        ],
        terminator: Terminator::Return(Some(ValueId(2))),
    }]
    .into();
    m.functions.push(caller);
    m
}

#[test]
fn static_utf8_satisfies_explicit_shared_life_without_fake_parameter_origin() {
    for stored in [false, true] {
        let m = bounded_module(stored);
        let c = candidate(&m);
        let checked = check(m, c, &Limits::default()).unwrap();
        assert_eq!(checked.result_origins(FunctionId(0)), Some(vec![]));
        assert_eq!(checked.result_has_static_origin(FunctionId(0)), Some(true));
        assert_eq!(checked.result_has_static_origin(FunctionId(999)), None);
    }
}
#[test]
fn static_utf8_checks_mixed_cfg_summary_as_real_union() {
    let m = mixed_module();
    let mut c = candidate(&m);
    for (f, p) in m.functions.iter().zip(&mut c.functions) {
        p.blocks = f
            .blocks
            .iter()
            .map(|block| BlockPlan {
                block: block.id,
                initialized_in: f.parameters.clone(),
                initialized_out: f.parameters.clone(),
            })
            .collect();
    }
    c.functions[0].holds[1].roots = vec![];
    c.functions[0].holds[2].roots = vec![HoldRoot::Parameter(0)];
    c.functions[1].holds[0].roots = vec![HoldRoot::Parameter(0)];
    c.functions[1].holds[1].roots = vec![];
    let mut expected = vec![HoldRoot::Static, HoldRoot::Parameter(0)];
    expected.sort();
    c.functions[1].holds[2].roots = expected;
    let checked = check(m.clone(), c.clone(), &Limits::default()).unwrap();
    assert_eq!(checked.result_origins(FunctionId(1)), Some(vec![0]));
    assert_eq!(checked.result_has_static_origin(FunctionId(1)), Some(true));
    c.functions[1].holds[2].roots = vec![HoldRoot::Static];
    assert!(check(m, c, &Limits::default()).is_err());
}
#[test]
fn static_utf8_forged_claim_cannot_ground_recursive_cycle() {
    let mut m = module();
    m.functions[0].blocks[0].instructions[0].kind = InstructionKind::Call {
        function: FunctionId(0),
        arguments: vec![],
    };
    let c = candidate(&m);
    assert!(check(m, c, &Limits::default()).is_err());
}
