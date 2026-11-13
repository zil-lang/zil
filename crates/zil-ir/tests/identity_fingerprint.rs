//! Identity equivalence only. The broad raw cases are not verifier capabilities.
use zil_base::{
    Endian, FileId, Fingerprint, ModuleId, NodeId, SCHEMA_VERSION, Span, TargetProfile,
};
use zil_ir::*;

#[path = "support/identity_instruction_cases.rs"]
mod cases;
#[path = "support/identity_reference_v48.rs"]
mod legacy;

fn types(flag: bool) -> Vec<Type> {
    vec![
        Type::Unit,
        Type::Bool,
        Type::I32,
        Type::I64,
        Type::U8,
        Type::U16,
        Type::ByteBuffer,
        Type::OwnedStore {
            element: Box::new(Type::I32),
        },
        Type::ForeignHandle,
        Type::CWideString,
        Type::ForeignCallback {
            parameters: vec![Type::U8],
            result: Box::new(Type::I64),
        },
        Type::Tuple(vec![Type::Bool, Type::I32]),
        Type::Array {
            element: Box::new(Type::U16),
            length: 23,
        },
        Type::Slice {
            element: Box::new(Type::I64),
        },
        Type::Named("名\0型".into()),
        Type::Record {
            name: "record".into(),
            fields: vec![("α".into(), Type::I32)],
        },
        Type::Package {
            name: "package".into(),
            fields: vec![("owned".into(), Type::Managed(Box::new(Type::ByteBuffer)))],
        },
        Type::Enum {
            name: "enum".into(),
            variants: vec![
                ("empty".into(), vec![]),
                ("full".into(), vec![Type::U8, Type::I64]),
            ],
        },
        Type::ErrorUnion {
            error: Box::new(Type::U8),
            result: Box::new(Type::I64),
        },
        Type::Address {
            mutable: flag,
            inner: Box::new(Type::I32),
        },
        Type::Managed(Box::new(Type::I64)),
        Type::Desc(Box::new(Type::I32)),
        Type::FunctionPointer {
            parameters: vec![Type::I32, Type::Bool],
            result: Box::new(Type::Unit),
        },
        Type::Closure(ClosureId(17)),
        Type::Opaque {
            identity: NodeId(29),
            hidden: Box::new(Type::I64),
        },
        Type::Unsupported("尚未支持".into()),
    ]
}
fn kinds() -> [CallableKind; 4] {
    [
        CallableKind::FunctionPointer,
        CallableKind::Fn,
        CallableKind::FnMut,
        CallableKind::FnOnce,
    ]
}
fn contract(flow: bool) -> CallableParameterContract {
    let calls = kinds()
        .into_iter()
        .enumerate()
        .map(|(i, kind)| CallEvent {
            parameter: i as u32,
            kind,
        })
        .collect::<Vec<_>>();
    CallableParameterContract {
        roles: kinds()
            .into_iter()
            .enumerate()
            .map(|(i, kind)| CallableRole {
                life: None,
                parameter: i as u32,
                closure: ClosureId(i as u32 + 2),
                kind,
                hold: vec![1, 3],
                call: vec![2, 4],
                writes: vec![5, 7],
                cleanup: if i % 2 == 0 {
                    CallableCleanup::Owned
                } else {
                    CallableCleanup::Borrowed
                },
            })
            .collect(),
        traces: if flow {
            CallableTraces::Flow(CallableFlow {
                entry: 0,
                nodes: vec![
                    CallableFlowNode {
                        calls: calls.clone(),
                        next: CallableFlowNext::Edges(vec![1, 2]),
                    },
                    CallableFlowNode {
                        calls: vec![],
                        next: CallableFlowNext::Exit(CallableExit::Return),
                    },
                    CallableFlowNode {
                        calls,
                        next: CallableFlowNext::Exit(CallableExit::Trap),
                    },
                ],
            })
        } else {
            CallableTraces::Finite(vec![
                CallableTrace {
                    calls,
                    exit: CallableExit::Return,
                },
                CallableTrace {
                    calls: vec![],
                    exit: CallableExit::Trap,
                },
            ])
        },
    }
}
fn return_contract(kind: CallableKind) -> CallableReturnContract {
    CallableReturnContract {
        life: None,
        capture_sources: Some(Vec::new()),
        source: zil_ir::CallableReturnSource::Fresh,
        closure: ClosureId(11),
        kind,
        hold: vec![1],
        call: vec![2],
        writes: vec![3],
        cleanup: vec![4, 6],
        lending_captures: vec![5, 7],
    }
}
fn module(flag: bool) -> TypedModule {
    let all_types = types(flag);
    assert_eq!(all_types.len(), 26);
    let mut operations = cases::instructions(flag);
    assert_eq!(operations.len(), 95);
    for value in [
        Constant::Unit,
        Constant::Bool(flag),
        Constant::I32(i32::MIN),
        Constant::I64(i64::MAX),
        Constant::U8(255),
        Constant::U16(65535),
        Constant::NullHandle,
        Constant::NullWideString,
        Constant::WideString(vec![0x03b1, 0xd83d, 0xde00, 0]),
    ] {
        operations.push(InstructionKind::Constant(value));
    }
    for op in [
        UnaryOp::Negate,
        UnaryOp::Not,
        UnaryOp::BitNot,
        UnaryOp::Dereference,
        UnaryOp::Borrow,
        UnaryOp::BorrowMut,
        UnaryOp::Give,
    ] {
        operations.push(InstructionKind::Unary {
            op,
            operand: ValueId(41),
        });
    }
    for op in [
        BinaryOp::Add,
        BinaryOp::Subtract,
        BinaryOp::Multiply,
        BinaryOp::Divide,
        BinaryOp::Remainder,
        BinaryOp::AddWrapping,
        BinaryOp::SubtractWrapping,
        BinaryOp::MultiplyWrapping,
        BinaryOp::Equal,
        BinaryOp::NotEqual,
        BinaryOp::Less,
        BinaryOp::LessEqual,
        BinaryOp::Greater,
        BinaryOp::GreaterEqual,
        BinaryOp::And,
        BinaryOp::Or,
        BinaryOp::BitAnd,
        BinaryOp::BitOr,
        BinaryOp::BitXor,
        BinaryOp::ShiftLeft,
        BinaryOp::ShiftRight,
    ] {
        operations.push(InstructionKind::Binary {
            op,
            left: ValueId(43),
            right: ValueId(47),
        });
    }
    for kind in [
        CleanupExitKind::Fallthrough,
        CleanupExitKind::Return,
        CleanupExitKind::Error,
        CleanupExitKind::Break,
        CleanupExitKind::Continue,
    ] {
        operations.push(InstructionKind::ScopeExit {
            scope: ScopeId(53),
            kind,
        });
    }
    for kind in kinds() {
        operations.push(InstructionKind::CallClosureBorrowed {
            callee: ValueId(59),
            kind,
            arguments: vec![ValueId(61), ValueId(67)],
        });
    }
    let instructions = operations
        .into_iter()
        .enumerate()
        .map(|(i, kind)| Instruction {
            result: (i % 2 == 0).then_some(ValueId(i as u32)),
            kind,
            span: Span::new(FileId(3), i * 2, i * 2 + 1),
        })
        .collect::<Vec<_>>();
    let functions = [
        PassingMode::Copy,
        PassingMode::Move,
        PassingMode::Describe,
        PassingMode::Create,
        PassingMode::Give,
        PassingMode::Borrow,
        PassingMode::BorrowMut,
        PassingMode::Infer,
    ]
    .into_iter()
    .enumerate()
    .map(|(i, mode)| {
        let terminators = [
            Terminator::Return(None),
            Terminator::Return(Some(ValueId(71))),
            Terminator::Jump(BlockId(2)),
            Terminator::Branch {
                condition: ValueId(73),
                then_block: BlockId(3),
                else_block: BlockId(5),
            },
            Terminator::Trap,
            Terminator::Unreachable,
        ];
        TypedFunction {
            dyn_instance: None,
            retained_return_sources: None,
            storage_life: None,
            call_kind: CallKind::Direct,
            resume_bound: crate::TaskResumeBound::Local,
            reference_return_bound: None,
            callable_return_contract: flag.then(|| return_contract(kinds()[i % 4])),
            callable_contract: flag.then(|| contract(i % 2 == 0)),
            protocol_contracts: Vec::new(),
            id: FunctionId(i as u32),
            name: format!("function-{i}-λ"),
            foreign: flag.then(|| ForeignDeclaration {
                abi: "C".into(),
                symbol: "foreign-名".into(),
                noescape_parameters: vec![2, 5],
                static_callbacks: i % 2 == 0,
                thread_driver_transfer: false,
                scoped_callback: None,
            }),
            environment: flag.then_some(ClosureId(i as u32)),
            parameters: vec![LocalId(0), LocalId(2)],
            return_type: all_types[i].clone(),
            result_mode: mode,
            locals: all_types
                .iter()
                .enumerate()
                .map(|(j, ty)| LocalDecl {
                    id: LocalId(j as u32),
                    name: format!("l{j}"),
                    ty: ty.clone(),
                    mutable: j % 2 == 0,
                    span: Span::new(FileId(7), j, j + 1),
                })
                .collect(),
            values: all_types.clone(),
            blocks: terminators
                .into_iter()
                .enumerate()
                .map(|(j, terminator)| BasicBlock {
                    id: BlockId(j as u32),
                    instructions: if j == 0 { instructions.clone() } else { vec![] },
                    terminator,
                })
                .collect(),
            entry: BlockId(0),
            span: Span::new(FileId(11), 13, 17),
        }
    })
    .collect();
    TypedModule {
        semantic: None,
        dyn_declarations: Vec::new(),
        dyn_interfaces: Vec::new(),
        dyn_vtables: Vec::new(),
        life_declarations: Vec::new(),
        life_instances: Vec::new(),
        schema_version: SCHEMA_VERSION,
        module_id: ModuleId(19),
        source_fingerprint: Fingerprint::of(b"cp34-current-source"),
        target: TargetProfile::host_llvm(),
        functions,
        closures: kinds()
            .into_iter()
            .enumerate()
            .map(|(i, kind)| ClosureDefinition {
                storage_life: None,
                id: ClosureId(i as u32),
                function: FunctionId(i as u32),
                captures: all_types.clone(),
                parameters: vec![Type::I32, Type::U8],
                result: Type::I64,
                kind,
                lending_captures: vec![1, 3],
            })
            .collect(),
        resources: vec![ResourceDefinition {
            name: "resource".into(),
            payload: Type::I64,
            teardown: flag.then_some(FunctionId(3)),
        }],
        unsupported: vec![
            UnsupportedFeature {
                feature: "raw-only".into(),
                span: None,
            },
            UnsupportedFeature {
                feature: "not-a-capability".into(),
                span: Some(Span::new(FileId(23), 29, 31)),
            },
        ],
    }
}
fn equal(module: &TypedModule) {
    // Keep the independent V48 encoder unchanged. For its explicitly empty
    // inventories, V49 changes only the domain and inserts the absent lexical
    // semantic program after the two original nominal-table counts.
    assert!(module.semantic.is_none());
    let mut current = legacy::bytes(module);
    let domain = b"ZIL-TYPED-MODULE-V49\0";
    current[..domain.len()].copy_from_slice(domain);
    let semantic_offset = domain.len() + 4 + 4 + 32 + 8 + module.target.to_json().len() + 16;
    current.insert(semantic_offset, 0);
    assert_eq!(module.fingerprint(), Fingerprint::of(&current));
}

#[test]
fn frozen_vec_oracle_and_archived_encoders_have_pinned_independent_sources() {
    let source = include_str!("support/identity_legacy_v31.rs");
    let (_, original) = source
        .split_once("// BEGIN FROZEN BASELINE ENCODER\n")
        .unwrap();
    assert_eq!(
        Fingerprint::of(original.as_bytes()).to_hex(),
        "96c82451fb7280e734046399bc51f1ed6e534bd6c3633e7ed519244ce4f41cab"
    );
    let (_, archived30) = include_str!("support/identity_legacy_v30.rs")
        .split_once("// BEGIN FROZEN BASELINE ENCODER\n")
        .unwrap();
    assert_eq!(
        Fingerprint::of(archived30.as_bytes()).to_hex(),
        "b66ed171400a682a28b20b1347b7499fdc57abcdb3525924c4aa017211084458"
    );
    let (_, archived29) = include_str!("support/identity_legacy_v29.rs")
        .split_once("// BEGIN FROZEN BASELINE ENCODER\n")
        .unwrap();
    assert_eq!(
        Fingerprint::of(archived29.as_bytes()).to_hex(),
        "03a8e0ee37770932c86d79274ae98d5d720fb06d2d4790f835b4dbdb7289e4a8"
    );
    let (_, previous) = include_str!("support/identity_legacy_v28.rs")
        .split_once("// BEGIN FROZEN BASELINE ENCODER\n")
        .unwrap();
    assert_eq!(
        Fingerprint::of(previous.as_bytes()).to_hex(),
        "18d902acfce7342c5f5d8ec797b282ebbeb120d3e649761b6c520795ed522b56"
    );
    let (_, archived) = include_str!("support/identity_legacy_v27.rs")
        .split_once("// BEGIN FROZEN BASELINE ENCODER\n")
        .unwrap();
    assert_eq!(
        Fingerprint::of(archived.as_bytes()).to_hex(),
        "51ca6f134a7af5e0abbd18a1f0320201c724240ad1faf64fd309b1b1f435526a"
    );
    for (name, expected) in [
        (
            "passing_mode_tag",
            "7649d5d1b89712ad86add09dde2e141b6ea99a5e98a15953d3002d2d7aaaedc4",
        ),
        (
            "cleanup_exit_tag",
            "5bbcb832d2786108ad7cc8cc6ba1f22cfd82fc688be339a631a3a6c409bb1d9d",
        ),
    ] {
        let begin = source.find(&format!("fn {name}(")).unwrap();
        let end = begin + source[begin..].find("\n}\n").unwrap() + 3;
        assert_eq!(
            Fingerprint::of(&source.as_bytes()[begin..end]).to_hex(),
            expected
        );
    }
}

#[test]
fn retained_legacy_variants_match_independent_vec_oracle_with_current_envelope() {
    for flag in [false, true] {
        let mut input = module(flag);
        equal(&input);
        input.target.endian = Endian::Big;
        input.target.pointer_width = 32;
        input.target.id = "target\0-α".into();
        input.target.checked_arithmetic = false;
        input.schema_version = 0x12345678;
        input.source_fingerprint = Fingerprint([0xa5; 32]);
        equal(&input);
        input.functions.reverse();
        equal(&input);
        input.functions.clear();
        input.closures.clear();
        input.resources.clear();
        input.unsupported.clear();
        equal(&input);
    }
}

#[test]
fn sha_blocks_and_stream_buffer_boundaries_preserve_exact_bytes() {
    let mut input = module(true);
    input.functions.truncate(1);
    let lengths = (0..=129)
        .chain(4090..=4102)
        .chain(8186..=8198)
        .chain([16384, 65537]);
    for length in lengths {
        input.functions[0].name = "x".repeat(length);
        equal(&input);
    }
}

#[test]
fn callable_public_fingerprints_keep_the_original_vec_contracts() {
    for flow in [false, true] {
        let value = contract(flow);
        assert_eq!(
            callable_contract_fingerprint(&value),
            legacy::callable_contract_fingerprint(&value)
        );
    }
    for kind in kinds() {
        let value = return_contract(kind);
        assert_eq!(
            callable_return_contract_fingerprint(&value),
            legacy::callable_return_contract_fingerprint(&value)
        );
    }
}

#[test]
fn domain_rejects_v27_identity_even_with_the_same_schema_claim() {
    let mut input = module(false);
    input.schema_version = 27;
    let mut old_domain = legacy::bytes_v31(&input);
    let domain = b"ZIL-TYPED-MODULE-V31\0";
    assert!(old_domain.starts_with(domain));
    old_domain[..domain.len()].copy_from_slice(b"ZIL-TYPED-MODULE-V27\0");
    assert_ne!(input.fingerprint(), Fingerprint::of(&old_domain));
}

#[test]
fn domain_rejects_previous_v28_even_with_same_schema_claim() {
    let mut input = module(false);
    input.schema_version = 28;
    let mut previous = legacy::bytes_v31(&input);
    previous[..b"ZIL-TYPED-MODULE-V31\0".len()].copy_from_slice(b"ZIL-TYPED-MODULE-V28\0");
    assert_ne!(input.fingerprint(), Fingerprint::of(&previous));
}

#[test]
fn owned_sum_domain_rejects_previous_v29_with_same_schema_claim() {
    let mut input = module(false);
    input.schema_version = 29;
    let mut previous = legacy::bytes_v31(&input);
    previous[..b"ZIL-TYPED-MODULE-V31\0".len()].copy_from_slice(b"ZIL-TYPED-MODULE-V29\0");
    assert_ne!(input.fingerprint(), Fingerprint::of(&previous));
}

#[test]
fn source_and_returned_debt_are_bound_in_all_public_identities() {
    let mut input = module(false);
    input.functions[0].callable_return_contract = Some(return_contract(CallableKind::Fn));
    input.functions[0].callable_contract = Some(contract(false));
    let fresh = input.fingerprint();
    let mut previous = legacy::bytes_v31(&input);
    previous[..b"ZIL-TYPED-MODULE-V31\0".len()].copy_from_slice(b"ZIL-TYPED-MODULE-V30\0");
    assert_ne!(fresh, Fingerprint::of(&previous));
    let mut identities = std::collections::BTreeSet::new();
    for source in [
        CallableReturnSource::Fresh,
        CallableReturnSource::Parameter(0),
        CallableReturnSource::Parameter(2),
        CallableReturnSource::Parameter(u32::MAX),
    ] {
        input.functions[0]
            .callable_return_contract
            .as_mut()
            .unwrap()
            .source = source;
        let returned = input.functions[0]
            .callable_return_contract
            .as_ref()
            .unwrap();
        assert_eq!(
            callable_return_contract_fingerprint(returned),
            legacy::callable_return_contract_fingerprint(returned)
        );
        assert!(identities.insert(format!(
            "{:?}",
            callable_return_contract_fingerprint(returned)
        )));
        equal(&input);
        if source != CallableReturnSource::Fresh {
            assert_ne!(fresh, input.fingerprint());
        }
    }
    let before = input.fingerprint();
    let roles = input.functions[0].callable_contract.as_mut().unwrap();
    let before_roles = callable_contract_fingerprint(roles);
    roles.roles[0].cleanup = CallableCleanup::Returned;
    assert_ne!(before_roles, callable_contract_fingerprint(roles));
    assert_eq!(
        callable_contract_fingerprint(roles),
        legacy::callable_contract_fingerprint(roles)
    );
    assert_ne!(before, input.fingerprint());
    equal(&input);
}

#[test]
fn wire_preserves_actual_parameter_and_rejects_unknown_source() {
    let limits = zil_base::Limits::default();
    let mut input = module(false);
    input.functions[0].callable_return_contract = Some(return_contract(CallableKind::Fn));
    input.functions[0].callable_contract = Some(contract(false));
    input.functions[0].callable_contract.as_mut().unwrap().roles[0].cleanup =
        CallableCleanup::Returned;
    let encode = |module: &TypedModule| {
        let plan = CandidatePlan::empty_for(module);
        let bytes = wire::encode_module_candidate(module, &plan, &limits).unwrap();
        assert_eq!(
            wire::decode_module_candidate(&bytes, &limits).unwrap(),
            (module.clone(), plan)
        );
        bytes
    };
    let fresh = encode(&input);
    input.functions[0]
        .callable_return_contract
        .as_mut()
        .unwrap()
        .source = CallableReturnSource::Parameter(2);
    let parameter = encode(&input);
    let tag = fresh
        .iter()
        .zip(&parameter)
        .position(|(a, b)| a != b)
        .unwrap();
    assert_eq!((fresh[tag], parameter[tag]), (0, 1));
    let mut malformed = parameter.clone();
    malformed[tag] = 255;
    assert_eq!(
        wire::decode_module_candidate(&malformed, &limits).unwrap_err()[0].code,
        "E-WIRE-TAG"
    );
    let mut old = parameter;
    old[8..12].copy_from_slice(&30u32.to_le_bytes());
    assert_eq!(
        wire::decode_module_candidate(&old, &limits).unwrap_err()[0].code,
        "E-WIRE-VERSION"
    );
}

#[test]
fn scoped32_reference_preserves_v31_anchors_and_rejects_new_old_forms() {
    for (flag, expected) in [
        (
            false,
            "cdba3011a81024ed776635765787f08b5101803bc484b766eb210c25fd7ea01d",
        ),
        (
            true,
            "df835877526332c329baf8f3f812865b2149c9031455ac4d11eac922af8b0695",
        ),
    ] {
        let mut input = module(flag);
        input.schema_version = 31;
        assert_eq!(
            Fingerprint::of(&legacy::bytes_v31(&input)).to_hex(),
            expected
        );
    }
    let mut input = module(true);
    for role in [
        None,
        Some(ScopedCallbackRole::Register),
        Some(ScopedCallbackRole::Close),
    ] {
        input.functions[0].foreign.as_mut().unwrap().scoped_callback = role;
        equal(&input);
        if role.is_some() {
            assert!(std::panic::catch_unwind(|| legacy::bytes_v31(&input)).is_err());
        }
    }
    input.functions[0].foreign.as_mut().unwrap().scoped_callback = None;
    input.functions[0].blocks[0].instructions.push(Instruction {
        result: Some(ValueId(37)),
        span: Span::default(),
        kind: InstructionKind::WithForeignCallback {
            register: FunctionId(11),
            close: FunctionId(23),
            callback: ValueId(41),
            body: ValueId(59),
            region: 73,
        },
    });
    equal(&input);
    assert!(std::panic::catch_unwind(|| legacy::bytes_v31(&input)).is_err());
    for field in 0..5 {
        let mut changed = input.clone();
        let InstructionKind::WithForeignCallback {
            register,
            close,
            callback,
            body,
            region,
        } = &mut changed.functions[0].blocks[0]
            .instructions
            .last_mut()
            .unwrap()
            .kind
        else {
            panic!()
        };
        match field {
            0 => register.0 += 1,
            1 => close.0 += 1,
            2 => callback.0 += 1,
            3 => body.0 += 1,
            _ => *region += 1,
        };
        equal(&changed);
        assert_ne!(input.fingerprint(), changed.fingerprint(), "field {field}");
    }
}

#[test]
fn life_and_scoped_thread_identity_fields_match_the_extended_reference() {
    let mut raw = module(false);
    raw.functions[0].reference_return_bound = Some(vec![0, 2]);
    equal(&raw);
    assert!(std::panic::catch_unwind(|| legacy::bytes_v31(&raw)).is_err());
    let original = raw.fingerprint();
    raw.functions[0].reference_return_bound = Some(vec![0, 3]);
    equal(&raw);
    assert_ne!(original, raw.fingerprint());
    raw.functions[0].blocks[0].instructions.push(Instruction {
        result: Some(ValueId(17)),
        span: Span::default(),
        kind: InstructionKind::WithScopedThread {
            worker: FunctionId(101),
            argument: ValueId(103),
            body: ValueId(107),
        },
    });
    equal(&raw);
    for field in 0..3 {
        let mut changed = raw.clone();
        let InstructionKind::WithScopedThread {
            worker,
            argument,
            body,
        } = &mut changed.functions[0].blocks[0]
            .instructions
            .last_mut()
            .unwrap()
            .kind
        else {
            panic!()
        };
        match field {
            0 => worker.0 += 1,
            1 => argument.0 += 1,
            _ => body.0 += 1,
        }
        equal(&changed);
        assert_ne!(raw.fingerprint(), changed.fingerprint());
    }
}

#[test]
fn numeric_sync_task_and_address_fields_match_reference_bytes() {
    let mut input = module(false);
    let address = Type::Address {
        mutable: true,
        inner: Box::new(Type::I32),
    };
    for ty in [
        Type::U64,
        Type::Usize,
        Type::F32,
        Type::F64,
        Type::TaskI32,
        Type::Registration {
            environment: Box::new(Type::Closure(ClosureId(17))),
        },
        Type::SyncCell(SyncCellKind::MutexI32),
        Type::SyncCell(SyncCellKind::AtomicI32),
        Type::BorrowedFunctionPointer {
            parameters: vec![address.clone()],
            result: Box::new(address),
            life: StorageLifeContract::from_flat(FunctionLifeContract {
                binder_count: 1,
                parameter_lives: vec![Some(0)],
                result_life: Some(0),
            })
            .unwrap(),
        },
    ] {
        input.functions[0].return_type = ty;
        equal(&input);
    }
    let path = vec![
        VariantProjection {
            variant: 3,
            index: 5,
        },
        VariantProjection {
            variant: 7,
            index: 11,
        },
    ];
    for kind in [
        InstructionKind::Constant(Constant::U64(u64::MAX)),
        InstructionKind::Constant(Constant::Usize(1_u64 << 63)),
        InstructionKind::Constant(Constant::F32(0x7fc0_1234)),
        InstructionKind::Constant(Constant::F64(0x8000_0000_0000_0000)),
        InstructionKind::ConvertNumber {
            value: ValueId(17),
            target: Type::F64,
        },
        InstructionKind::WithSyncCell {
            kind: SyncCellKind::AtomicI32,
            initial: ValueId(13),
            body: ValueId(19),
        },
        InstructionKind::SyncI32 {
            op: SyncI32Op::FetchAdd,
            address: ValueId(23),
            value: Some(ValueId(29)),
            ordering: AtomicOrdering::AcqRel,
        },
        InstructionKind::TaskStart {
            worker: FunctionId(31),
            argument: ValueId(37),
        },
        InstructionKind::RegistrationStart {
            register: FunctionId(97),
            close: FunctionId(101),
            callback: ValueId(103),
            region: 107,
        },
        InstructionKind::RegistrationValid {
            registration: ValueId(109),
        },
        InstructionKind::RegistrationClose {
            registration: ValueId(113),
        },
        InstructionKind::TaskYield,
        InstructionKind::TaskValid { task: ValueId(41) },
        InstructionKind::TaskPoll { task: ValueId(43) },
        InstructionKind::TaskRequestCancel { task: ValueId(47) },
        InstructionKind::TaskPollClose { task: ValueId(53) },
        InstructionKind::TaskFinishDestroy { task: ValueId(59) },
        InstructionKind::BorrowVariantLocal {
            local: LocalId(61),
            path: path.clone(),
            mutable: true,
        },
        InstructionKind::VariantTagAddress {
            address: ValueId(67),
            path: path.clone(),
        },
        InstructionKind::BorrowVariantAddress {
            address: ValueId(71),
            path,
            mutable: false,
        },
        InstructionKind::DescribeAddress {
            address: ValueId(89),
        },
        InstructionKind::BorrowFieldLocal {
            local: LocalId(73),
            path: vec![79, 83],
            mutable: true,
        },
    ]
    .into_iter()
    .chain(cases::terminal_instructions(false))
    .chain(cases::terminal_instructions(true))
    {
        input.functions[0].blocks[0].instructions.push(Instruction {
            result: None,
            span: Span::default(),
            kind,
        });
        equal(&input);
    }
}

#[test]
fn higher_rank_and_suspend_fields_match_independent_reference() {
    let mut input = module(false);
    input.functions[0].call_kind = CallKind::Suspend;
    input.functions[0].callable_contract = Some(contract(false));
    input.functions[0].callable_contract.as_mut().unwrap().roles[0].life =
        StorageLifeContract::from_flat(FunctionLifeContract {
            binder_count: 2,
            parameter_lives: vec![Some(0), None, Some(1)],
            result_life: Some(0),
        });
    input.functions[0].blocks[0].instructions.push(Instruction {
        result: Some(ValueId(131)),
        span: Span::default(),
        kind: InstructionKind::SuspendCall {
            function: FunctionId(137),
            arguments: vec![ValueId(139), ValueId(149)],
        },
    });
    equal(&input);
    assert!(std::panic::catch_unwind(|| legacy::bytes_v31(&input)).is_err());
    for change in 0..7 {
        let mut changed = input.clone();
        let f = &mut changed.functions[0];
        if change == 0 {
            f.call_kind = CallKind::Direct;
        } else if change <= 4 {
            let role = &mut f.callable_contract.as_mut().unwrap().roles[0];
            match change {
                1 => role.life = None,
                2 => role.life.as_mut().unwrap().invocation_binder_count += 1,
                3 => role.life.as_mut().unwrap().bindings.swap(0, 1),
                _ => {
                    role.life
                        .as_mut()
                        .unwrap()
                        .bindings
                        .last_mut()
                        .unwrap()
                        .binder = StorageBinder::Invocation(1)
                }
            }
        } else {
            let InstructionKind::SuspendCall {
                function,
                arguments,
            } = &mut f.blocks[0].instructions.last_mut().unwrap().kind
            else {
                unreachable!()
            };
            if change == 5 {
                function.0 += 1;
            } else {
                arguments.swap(0, 1);
            }
        }
        equal(&changed);
        assert_ne!(input.fingerprint(), changed.fingerprint());
    }
}

#[test]
fn returned_capture_sources_bind_identity_including_unknown_empty_and_actual_paths() {
    use zil_ir::{
        ContractProjection, PlaceProjection, StorageBase, StoragePosition, StorageSourceBinding,
    };
    let mut contract = return_contract(CallableKind::Fn);
    let empty = zil_ir::callable_return_contract_fingerprint(&contract);
    contract.capture_sources = None;
    assert_ne!(
        empty,
        zil_ir::callable_return_contract_fingerprint(&contract)
    );
    contract.capture_sources = Some(vec![StorageSourceBinding {
        position: StoragePosition {
            base: StorageBase::Capture(0),
            path: vec![ContractProjection::Capture(1)],
        },
        sources: (vec![StoragePosition {
            base: StorageBase::Parameter(0),
            path: vec![ContractProjection::Place(PlaceProjection::Field(0))],
        }])
        .into_iter()
        .map(zil_ir::StorageSource::Exact)
        .collect(),
    }]);
    let actual = zil_ir::callable_return_contract_fingerprint(&contract);
    assert_ne!(empty, actual);
    exact_source_mut(&mut contract.capture_sources.as_mut().unwrap()[0].sources[0]).base =
        StorageBase::Parameter(1);
    assert_ne!(
        actual,
        zil_ir::callable_return_contract_fingerprint(&contract)
    );
}

#[test]
fn returned_source_wire_preserves_minimal_positions_unknown_and_empty() {
    let limits = zil_base::Limits::default();
    for sources in [
        None,
        Some(vec![]),
        Some(vec![StorageSourceBinding {
            position: StoragePosition {
                base: StorageBase::Result,
                path: vec![],
            },
            sources: (vec![StoragePosition {
                base: StorageBase::Result,
                path: vec![],
            }])
            .into_iter()
            .map(zil_ir::StorageSource::Exact)
            .collect(),
        }]),
        Some(vec![StorageSourceBinding {
            position: StoragePosition {
                base: StorageBase::Capture(0),
                path: vec![ContractProjection::Capture(1)],
            },
            sources: (vec![StoragePosition {
                base: StorageBase::Parameter(1),
                path: vec![ContractProjection::Place(PlaceProjection::Field(2))],
            }])
            .into_iter()
            .map(zil_ir::StorageSource::Exact)
            .collect(),
        }]),
    ] {
        // Transport controls deliberately assert no semantic kernel admission.
        let mut raw = module(false);
        let mut c = return_contract(CallableKind::Fn);
        c.capture_sources = sources;
        raw.functions[0].callable_return_contract = Some(c);
        let plan = CandidatePlan::empty_for(&raw);
        let bytes = wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
        let decoded = wire::decode_module_candidate(&bytes, &limits).unwrap();
        assert_eq!(raw, decoded.0);
        assert!(wire::decode_module_candidate(&bytes[..bytes.len() - 1], &limits).is_err());
        let mut previous = bytes.clone();
        previous[8..12].copy_from_slice(&47u32.to_le_bytes());
        assert!(wire::decode_module_candidate(&previous, &limits).is_err());
    }
}

#[test]
fn receiver_head_has_distinct_identity_and_lossless_raw_transport() {
    let limits = zil_base::Limits::default();
    let position = |base| StoragePosition {
        base,
        path: Vec::new(),
    };
    let mut life = StorageLifeContract {
        static_bindings: Vec::new(),
        fixed_binder_count: 0,
        invocation_binder_count: 1,
        bindings: vec![StorageLifeBinding {
            position: position(StorageBase::Receiver),
            binder: StorageBinder::Invocation(0),
        }],
        classes: Vec::new(),
    };
    assert!(life.to_flat(0).is_none());
    let receiver = life.fingerprint();
    for base in [
        StorageBase::Parameter(0),
        StorageBase::Capture(0),
        StorageBase::Result,
    ] {
        life.bindings[0].position.base = base;
        assert_ne!(receiver, life.fingerprint());
    }
    life.bindings[0].position.base = StorageBase::Receiver;
    // Transport is intentionally untrusted and does not assert admission.
    let mut raw = module(false);
    use zil_ir::ct_artifact::{CtScalarTy, CtTypeNode, CtTypeNodeId, CtTypeRef, CtValue};
    let ct = CtValue::Type(CtTypeRef {
        scoped_catalogue: None,
        ambient_classes: Vec::new(),
        nodes: vec![
            CtTypeNode::Primitive(CtScalarTy::I32),
            CtTypeNode::BorrowedFunctionPointer {
                parameters: Vec::new(),
                result: CtTypeNodeId(0),
                life: life.clone(),
            },
        ],
        root: CtTypeNodeId(1),
    });
    let ct_bytes = zil_ir::ct_wire::encode_value(&ct, &limits).unwrap();
    assert_eq!(
        zil_ir::ct_wire::decode_value(&ct_bytes, &limits).unwrap(),
        ct
    );
    raw.functions[0].storage_life = Some(life);
    raw.functions[0].retained_return_sources = Some(vec![StorageSourceBinding {
        position: position(StorageBase::Result),
        sources: (vec![position(StorageBase::Receiver)])
            .into_iter()
            .map(zil_ir::StorageSource::Exact)
            .collect(),
    }]);
    let plan = CandidatePlan::empty_for(&raw);
    let bytes = wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
    assert_eq!(
        wire::decode_module_candidate(&bytes, &limits).unwrap().0,
        raw
    );
}

#[test]
fn callable_return_expected_life_is_distinct_and_lossless() {
    let limits = zil_base::Limits::default();
    let mut raw = module(false);
    let mut returned = return_contract(CallableKind::FnOnce);
    let absent = callable_return_contract_fingerprint(&returned);
    let relation = StorageLifeContract {
        static_bindings: Vec::new(),
        fixed_binder_count: 2,
        invocation_binder_count: 0,
        bindings: vec![StorageLifeBinding {
            position: StoragePosition {
                base: StorageBase::Result,
                path: vec![],
            },
            binder: StorageBinder::Fixed(1),
        }],
        classes: vec![],
    };
    returned.life = Some(relation);
    let present = callable_return_contract_fingerprint(&returned);
    assert_ne!(absent, present);
    assert_eq!(
        present,
        legacy::callable_return_contract_fingerprint(&returned)
    );
    for binder in [StorageBinder::Fixed(0), StorageBinder::Invocation(0)] {
        let mut forged = returned.clone();
        forged.life.as_mut().unwrap().bindings[0].binder = binder;
        assert_ne!(present, callable_return_contract_fingerprint(&forged));
    }
    raw.functions[0].callable_return_contract = Some(returned);
    let candidate = CandidatePlan::empty_for(&raw);
    let bytes = wire::encode_module_candidate(&raw, &candidate, &limits).unwrap();
    let (decoded, _) = wire::decode_module_candidate(&bytes, &limits).unwrap();
    assert_eq!(raw, decoded);
    assert!(wire::decode_module_candidate(&bytes[..bytes.len() - 1], &limits).is_err());
}

fn exact_source_mut(source: &mut zil_ir::StorageSource) -> &mut zil_ir::StoragePosition {
    match source {
        zil_ir::StorageSource::Exact(position) => position,
        _ => panic!("fixture requires an exact source"),
    }
}
