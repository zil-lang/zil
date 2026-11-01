//! Identity equivalence only. The broad raw cases are not verifier capabilities.
use zil_base::{
    Endian, FileId, Fingerprint, ModuleId, NodeId, SCHEMA_VERSION, Span, TargetProfile,
};
use zil_ir::*;

#[path = "support/cp34_instruction_cases.rs"]
mod cases;
#[path = "support/cp34_legacy_v29.rs"]
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
    assert_eq!(operations.len(), 91);
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
            callable_return_contract: flag.then(|| return_contract(kinds()[i % 4])),
            callable_contract: flag.then(|| contract(i % 2 == 0)),
            id: FunctionId(i as u32),
            name: format!("function-{i}-λ"),
            foreign: flag.then(|| ForeignDeclaration {
                abi: "C".into(),
                symbol: "foreign-名".into(),
                noescape_parameters: vec![2, 5],
                static_callbacks: i % 2 == 0,
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
        schema_version: SCHEMA_VERSION,
        module_id: ModuleId(19),
        source_fingerprint: Fingerprint::of(b"cp34-current-source"),
        target: TargetProfile::host_llvm(),
        functions,
        closures: kinds()
            .into_iter()
            .enumerate()
            .map(|(i, kind)| ClosureDefinition {
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
    assert_eq!(
        module.fingerprint(),
        Fingerprint::of(&legacy::bytes(module))
    );
}

#[test]
fn frozen_oracle_is_the_original_encoder_not_the_new_sink() {
    let source = include_str!("support/cp34_legacy_v29.rs");
    let (_, original) = source
        .split_once("// BEGIN FROZEN BASELINE ENCODER\n")
        .unwrap();
    assert_eq!(
        Fingerprint::of(original.as_bytes()).to_hex(),
        "03a8e0ee37770932c86d79274ae98d5d720fb06d2d4790f835b4dbdb7289e4a8"
    );
    let (_, previous) = include_str!("support/cp34_legacy_v28.rs")
        .split_once("// BEGIN FROZEN BASELINE ENCODER\n")
        .unwrap();
    assert_eq!(
        Fingerprint::of(previous.as_bytes()).to_hex(),
        "18d902acfce7342c5f5d8ec797b282ebbeb120d3e649761b6c520795ed522b56"
    );
    let (_, archived) = include_str!("support/cp34_legacy_v27.rs")
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
fn every_raw_encoding_variant_matches_the_frozen_vec_oracle() {
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
fn defer227_domain_rejects_v27_identity_even_with_the_same_schema_claim() {
    let mut input = module(false);
    input.schema_version = 27;
    let mut old_domain = legacy::bytes(&input);
    let domain = b"ZIL-TYPED-MODULE-V29\0";
    assert!(old_domain.starts_with(domain));
    old_domain[..domain.len()].copy_from_slice(b"ZIL-TYPED-MODULE-V27\0");
    assert_ne!(input.fingerprint(), Fingerprint::of(&old_domain));
}

#[test]
fn product237_domain_rejects_previous_v28_even_with_same_schema_claim() {
    let mut input = module(false);
    input.schema_version = 28;
    let mut previous = legacy::bytes(&input);
    previous[..b"ZIL-TYPED-MODULE-V29\0".len()].copy_from_slice(b"ZIL-TYPED-MODULE-V28\0");
    assert_ne!(input.fingerprint(), Fingerprint::of(&previous));
}
