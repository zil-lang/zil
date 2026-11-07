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
            locals: vec![],
            values: vec![address()],
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
            }],
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
    }];
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
    }];
    f.values = vec![address(), address(), address()];
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

#[test]
fn static_utf8_reconstructs_real_static_root_across_helper_and_dead_pointer_slot() {
    let m = helper_module();
    let p = analyze(&m, &Limits::default()).unwrap();
    assert_eq!(p.functions[0].holds[0].roots, vec![HoldRoot::Static]);
    for h in &p.functions[1].holds {
        assert_eq!(h.roots, vec![HoldRoot::Static]);
    }
}
#[test]
fn static_utf8_remains_static_in_stored_relation_modules() {
    let m = stored_module();
    let p = analyze(&m, &Limits::default()).unwrap();
    assert_eq!(p.functions[0].holds[0].roots, vec![HoldRoot::Static]);
}
#[test]
fn static_utf8_accepts_empty_and_embedded_nul_without_normalizing() {
    for bytes in [vec![], vec![0], "e\u{301}".as_bytes().to_vec()] {
        let mut m = module();
        m.functions[0].blocks[0].instructions[0].kind = InstructionKind::StaticUtf8 { bytes };
        assert_eq!(
            analyze(&m, &Limits::default()).unwrap().functions[0].holds[0].roots,
            vec![HoldRoot::Static]
        );
    }
}
#[test]
fn static_utf8_rejects_invalid_encoding_and_exact_result_forgery() {
    for bytes in [
        vec![0xff],
        vec![0xc0, 0x80],
        vec![0xed, 0xa0, 0x80],
        vec![0xf4, 0x90, 0x80, 0x80],
        vec![0xe2, 0x82],
    ] {
        let mut m = module();
        m.functions[0].blocks[0].instructions[0].kind = InstructionKind::StaticUtf8 { bytes };
        assert_eq!(
            analyze(&m, &Limits::default()).unwrap_err()[0].code,
            "E-ANALYZE-IR"
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
        assert!(
            analyze(&m, &Limits::default()).unwrap_err()[0]
                .message
                .contains("immutable byte slice")
        );
    }
    let mut m = module();
    m.functions[0].blocks[0].instructions[0].result = None;
    assert!(analyze(&m, &Limits::default()).is_err());
}
#[test]
fn static_utf8_rejects_mutable_reborrow_even_without_a_write() {
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
        assert!(analyze(&m, &Limits::default()).is_err());
    }
}
#[test]
fn static_utf8_charges_raw_bytes_and_current_request_budget() {
    let m = module();
    let limits = Limits {
        max_nodes: 2,
        ..Limits::default()
    };
    assert_eq!(
        analyze(&m, &limits).unwrap_err()[0].category,
        zil_base::ResultCategory::ResourceLimit
    );
    let limits = Limits {
        max_analysis_steps: 1,
        ..Limits::default()
    };
    assert_eq!(
        analyze(&m, &limits).unwrap_err()[0].category,
        zil_base::ResultCategory::ResourceLimit
    );
    assert!(analyze(&m, &Limits::default()).is_ok());
    assert!(analyze(&m, &limits).is_err());
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
    }];
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
    caller.values = vec![address(), Type::Bool, address()];
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
    }];
    m.functions.push(caller);
    m
}

#[test]
fn static_utf8_satisfies_shared_explicit_life_upper_in_both_profiles() {
    for stored in [false, true] {
        let m = bounded_module(stored);
        let p = analyze(&m, &Limits::default()).unwrap();
        assert_eq!(p.functions[0].holds[0].roots, vec![HoldRoot::Static]);
    }
}
#[test]
fn static_utf8_unions_static_and_parameter_returns_without_replacing_parameter() {
    let m = mixed_module();
    let p = analyze(&m, &Limits::default()).unwrap();
    assert_eq!(p.functions[0].holds[2].roots, vec![HoldRoot::Parameter(0)]);
    let mut expected = vec![HoldRoot::Static, HoldRoot::Parameter(0)];
    expected.sort();
    assert_eq!(p.functions[1].holds[2].roots, expected);
}
#[test]
fn static_utf8_does_not_ground_an_unrelated_recursive_return() {
    let mut m = module();
    let f = &mut m.functions[0];
    f.blocks[0].instructions[0].kind = InstructionKind::Call {
        function: FunctionId(0),
        arguments: vec![],
    };
    let p = analyze(&m, &Limits::default()).unwrap();
    assert!(p.functions[0].holds[0].roots.is_empty());
}
