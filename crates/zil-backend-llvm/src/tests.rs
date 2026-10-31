//! These are hand-authored typed-IR/backend tests. Every fixture passes through
//! the actual independent verifier. They are not source-language acceptance.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

use zil_base::{
    Fingerprint, Limits, ModuleId, ResultCategory, SCHEMA_VERSION, Span, TargetProfile,
};
use zil_ir::*;
use zil_verify::{VerifiedModule, verify_module};

use crate::*;

fn instruction(result: Option<u32>, kind: InstructionKind) -> Instruction {
    Instruction {
        result: result.map(ValueId),
        kind,
        span: Span::default(),
    }
}
fn constant(result: u32, value: Constant) -> Instruction {
    instruction(Some(result), InstructionKind::Constant(value))
}
fn binary(result: u32, op: BinaryOp, left: u32, right: u32) -> Instruction {
    instruction(
        Some(result),
        InstructionKind::Binary {
            op,
            left: ValueId(left),
            right: ValueId(right),
        },
    )
}
fn load(result: u32, local: u32) -> Instruction {
    instruction(Some(result), InstructionKind::Load(LocalId(local)))
}
fn store(local: u32, value: u32) -> Instruction {
    instruction(
        None,
        InstructionKind::Store {
            local: LocalId(local),
            value: ValueId(value),
        },
    )
}
fn block(id: u32, instructions: Vec<Instruction>, terminator: Terminator) -> BasicBlock {
    BasicBlock {
        id: BlockId(id),
        instructions,
        terminator,
    }
}
fn local(id: u32, ty: Type, mutable: bool) -> LocalDecl {
    LocalDecl {
        id: LocalId(id),
        name: format!("local{id}"),
        ty,
        mutable,
        span: Span::default(),
    }
}
fn function(
    id: u32,
    name: &str,
    return_type: Type,
    values: Vec<Type>,
    blocks: Vec<BasicBlock>,
) -> TypedFunction {
    TypedFunction {
        callable_return_contract: None,
        callable_contract: None,
        result_mode: zil_ir::PassingMode::Infer,
        foreign: None,
        id: FunctionId(id),
        name: name.into(),
        environment: None,
        parameters: Vec::new(),
        return_type,
        locals: Vec::new(),
        values,
        blocks,
        entry: BlockId(0),
        span: Span::default(),
    }
}
fn returning(value: Constant) -> TypedFunction {
    function(
        0,
        "check",
        value.ty(),
        vec![value.ty()],
        vec![block(
            0,
            vec![constant(0, value)],
            Terminator::Return(Some(ValueId(0))),
        )],
    )
}
fn binary_function(op: BinaryOp, left: Constant, right: Constant) -> TypedFunction {
    let result_type = if matches!(
        op,
        BinaryOp::Equal
            | BinaryOp::NotEqual
            | BinaryOp::Less
            | BinaryOp::LessEqual
            | BinaryOp::Greater
            | BinaryOp::GreaterEqual
    ) {
        Type::Bool
    } else {
        left.ty()
    };
    function(
        0,
        "check",
        result_type.clone(),
        vec![left.ty(), right.ty(), result_type],
        vec![block(
            0,
            vec![constant(0, left), constant(1, right), binary(2, op, 0, 1)],
            Terminator::Return(Some(ValueId(2))),
        )],
    )
}

type BoundaryClaims = Vec<Vec<(Vec<u32>, Vec<u32>)>>;

/// Claims are supplied by each fixture, not copied out of the verifier or
/// obtained through its implementation. An incorrect claim fails the test.
fn checked(functions: Vec<TypedFunction>, claims: BoundaryClaims) -> VerifiedModule {
    checked_with_closures(functions, vec![], claims)
}
fn checked_with_closures(
    functions: Vec<TypedFunction>,
    closures: Vec<ClosureDefinition>,
    claims: BoundaryClaims,
) -> VerifiedModule {
    let module = TypedModule {
        resources: Vec::new(),
        schema_version: SCHEMA_VERSION,
        module_id: ModuleId(17),
        source_fingerprint: Fingerprint::of(b"hand-authored backend test fixture"),
        target: TargetProfile::host_llvm(),
        functions,
        closures,
        unsupported: Vec::new(),
    };
    let mut plan = CandidatePlan::empty_for(&module);
    assert_eq!(claims.len(), module.functions.len());
    for (function, boundaries) in module.functions.iter().zip(claims) {
        assert_eq!(boundaries.len(), function.blocks.len());
        plan.functions.push(FunctionPlan {
            function: function.id,
            holds: (0..function.values.len())
                .map(|i| zil_ir::ValueHold {
                    value: ValueId(i as u32),
                    roots: vec![],
                })
                .collect(),
            blocks: boundaries
                .into_iter()
                .enumerate()
                .map(|(index, (input, output))| BlockPlan {
                    block: BlockId(index as u32),
                    initialized_in: input.into_iter().map(LocalId).collect(),
                    initialized_out: output.into_iter().map(LocalId).collect(),
                })
                .collect(),
        });
    }
    let target = module.target.clone();
    verify_module(module, plan, &target, &Limits::default())
        .unwrap_or_else(|errors| panic!("fixture failed actual verification: {errors:?}"))
}
fn checked_simple(function: TypedFunction) -> VerifiedModule {
    let boundaries = function
        .blocks
        .iter()
        .map(|_| (Vec::new(), Vec::new()))
        .collect();
    checked(vec![function], vec![boundaries])
}
fn lir(verified: &VerifiedModule) -> LirModule {
    LirModule {
        resources: Vec::new(),
        schema_version: SCHEMA_VERSION,
        module_id: verified.module().module_id,
        verified_fingerprint: verified.fingerprint(),
        target: verified.module().target.clone(),
        functions: verified.module().functions.clone(),
        closures: verified.module().closures.clone(),
        layouts: vec![
            ScalarLayout {
                ty: Type::Unit,
                size: 0,
                alignment: 1,
            },
            ScalarLayout {
                ty: Type::Bool,
                size: 1,
                alignment: 1,
            },
            ScalarLayout {
                ty: Type::I32,
                size: 4,
                alignment: 4,
            },
            ScalarLayout {
                ty: Type::I64,
                size: 8,
                alignment: 8,
            },
            ScalarLayout {
                ty: Type::U8,
                size: 1,
                alignment: 1,
            },
            ScalarLayout {
                ty: Type::U16,
                size: 2,
                alignment: 2,
            },
            ScalarLayout {
                ty: Type::ForeignHandle,
                size: 8,
                alignment: 8,
            },
            ScalarLayout {
                ty: Type::CWideString,
                size: 8,
                alignment: 8,
            },
        ],
    }
}

static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(0);
struct TestDirectory(PathBuf);
impl TestDirectory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "zil-backend-test-{}-{}",
            std::process::id(),
            TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).expect("create unique backend test directory");
        Self(path)
    }
}
impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn native(verified: &VerifiedModule, entry: EntryPoint, optimization: Optimization) -> Output {
    let directory = TestDirectory::new();
    let mut options = BuildOptions::new(directory.0.join("native"));
    options.emit.entry = entry;
    options.optimization = optimization;
    options.clang = std::env::var_os("ZIL_CLANG")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("clang"));
    let artifact = build(verified, &lir(verified), &options)
        .unwrap_or_else(|errors| panic!("real LLVM build failed: {errors:?}"));
    assert!(artifact.llvm_ir.is_file());
    assert!(artifact.command.iter().any(|argument| argument == "ir"));
    Command::new(artifact.executable)
        .output()
        .expect("execute the real LLVM-produced native program")
}
fn returns(
    verified: &VerifiedModule,
    entry: FunctionId,
    expected: &str,
    optimization: Optimization,
) {
    let output = native(verified, EntryPoint::ReturnValue(entry), optimization);
    assert!(
        output.status.success(),
        "native execution failed: {:?}",
        output
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        format!("{expected}\n")
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn native_harness_observes_full_signed_results_at_both_optimization_levels() {
    for optimization in [Optimization::O0, Optimization::O2] {
        returns(
            &checked_simple(returning(Constant::I64(1_099_511_628_800))),
            FunctionId(0),
            "1099511628800",
            optimization,
        );
        returns(
            &checked_simple(returning(Constant::I32(-1024))),
            FunctionId(0),
            "-1024",
            optimization,
        );
        returns(
            &checked_simple(returning(Constant::Bool(true))),
            FunctionId(0),
            "1",
            optimization,
        );
    }
}

#[test]
fn synchronous_error_main_maps_both_tags_and_erased_success_at_o0_o2() {
    for success in [Type::I32, Type::Unit] {
        for is_error in [false, true] {
            let payload = if is_error {
                Constant::Bool(true)
            } else if success == Type::I32 {
                Constant::I32(42)
            } else {
                Constant::Unit
            };
            let union = Type::ErrorUnion {
                error: Box::new(Type::Bool),
                result: Box::new(success.clone()),
            };
            let fixture = function(
                0,
                "main",
                union.clone(),
                vec![payload.ty(), union],
                vec![block(
                    0,
                    vec![
                        instruction(None, InstructionKind::ScopeEnter { scope: ScopeId(0) }),
                        constant(0, payload),
                        instruction(
                            Some(1),
                            InstructionKind::MakeErrorUnion {
                                is_error,
                                value: ValueId(0),
                            },
                        ),
                        instruction(
                            None,
                            InstructionKind::ScopeExit {
                                scope: ScopeId(0),
                                kind: CleanupExitKind::Return,
                            },
                        ),
                    ],
                    Terminator::Return(Some(ValueId(1))),
                )],
            );
            let verified = checked_simple(fixture);
            for optimization in [Optimization::O0, Optimization::O2] {
                let output = native(&verified, EntryPoint::Main, optimization);
                let expected = if is_error {
                    1
                } else if success == Type::I32 {
                    42
                } else {
                    0
                };
                assert_eq!(output.status.code(), Some(expected), "{output:?}");
                assert!(output.stdout.is_empty());
                assert!(output.stderr.is_empty());
            }
        }
    }
}

#[test]
fn error_projection_tag_mismatch_traps_including_unit_payload() {
    for success in [Type::I32, Type::Unit] {
        let union = Type::ErrorUnion {
            error: Box::new(Type::Bool),
            result: Box::new(success.clone()),
        };
        let fixture = function(
            0,
            "check",
            Type::I32,
            vec![Type::Bool, union, success, Type::I32],
            vec![block(
                0,
                vec![
                    instruction(None, InstructionKind::ScopeEnter { scope: ScopeId(0) }),
                    constant(0, Constant::Bool(true)),
                    instruction(
                        Some(1),
                        InstructionKind::MakeErrorUnion {
                            is_error: true,
                            value: ValueId(0),
                        },
                    ),
                    instruction(
                        Some(2),
                        InstructionKind::ReadErrorUnion {
                            value: ValueId(1),
                            is_error: false,
                        },
                    ),
                    constant(3, Constant::I32(42)),
                    instruction(
                        None,
                        InstructionKind::ScopeExit {
                            scope: ScopeId(0),
                            kind: CleanupExitKind::Return,
                        },
                    ),
                ],
                Terminator::Return(Some(ValueId(3))),
            )],
        );
        let verified = checked_simple(fixture);
        for optimization in [Optimization::O0, Optimization::O2] {
            let output = native(
                &verified,
                EntryPoint::ReturnValue(FunctionId(0)),
                optimization,
            );
            assert!(!output.status.success(), "{output:?}");
            #[cfg(unix)]
            {
                use std::os::unix::process::ExitStatusExt;
                assert!(output.status.signal().is_some(), "{output:?}");
            }
            assert!(output.stdout.is_empty());
        }
    }
}

#[test]
fn error_union_nested_in_enum_preserves_wide_and_unit_payload_layout() {
    let payload = Type::Tuple(vec![Type::I64, Type::Unit, Type::Bool]);
    let union = Type::ErrorUnion {
        error: Box::new(Type::I32),
        result: Box::new(payload.clone()),
    };
    let holder = Type::Enum {
        name: "Holder".into(),
        variants: vec![("Value".into(), vec![union.clone()])],
    };
    let fixture = function(
        0,
        "check",
        Type::I64,
        vec![
            Type::I64,
            Type::Unit,
            Type::Bool,
            payload.clone(),
            union.clone(),
            holder,
            union,
            payload,
            Type::I64,
        ],
        vec![block(
            0,
            vec![
                instruction(None, InstructionKind::ScopeEnter { scope: ScopeId(0) }),
                constant(0, Constant::I64(5_000_000_042)),
                constant(1, Constant::Unit),
                constant(2, Constant::Bool(true)),
                instruction(
                    Some(3),
                    InstructionKind::MakeAggregate {
                        fields: vec![ValueId(0), ValueId(1), ValueId(2)],
                    },
                ),
                instruction(
                    Some(4),
                    InstructionKind::MakeErrorUnion {
                        is_error: false,
                        value: ValueId(3),
                    },
                ),
                instruction(
                    Some(5),
                    InstructionKind::MakeVariant {
                        variant: 0,
                        fields: vec![ValueId(4)],
                    },
                ),
                instruction(
                    Some(6),
                    InstructionKind::ReadVariant {
                        value: ValueId(5),
                        variant: 0,
                        index: 0,
                    },
                ),
                instruction(
                    Some(7),
                    InstructionKind::ReadErrorUnion {
                        value: ValueId(6),
                        is_error: false,
                    },
                ),
                instruction(
                    Some(8),
                    InstructionKind::ExtractField {
                        aggregate: ValueId(7),
                        index: 0,
                    },
                ),
                instruction(
                    None,
                    InstructionKind::ScopeExit {
                        scope: ScopeId(0),
                        kind: CleanupExitKind::Return,
                    },
                ),
            ],
            Terminator::Return(Some(ValueId(8))),
        )],
    );
    let verified = checked_simple(fixture);
    for optimization in [Optimization::O0, Optimization::O2] {
        returns(&verified, FunctionId(0), "5000000042", optimization);
    }
}

#[test]
fn ordinary_main_uses_real_process_exit_status_and_unit_returns_zero() {
    let mut main = returning(Constant::I32(7));
    main.name = "main".into();
    let output = native(&checked_simple(main), EntryPoint::Main, Optimization::O0);
    assert_eq!(output.status.code(), Some(7));
    assert!(output.stdout.is_empty());
    let mut unit_main = returning(Constant::Unit);
    unit_main.name = "main".into();
    let output = native(
        &checked_simple(unit_main),
        EntryPoint::Main,
        Optimization::O2,
    );
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty());
}

#[test]
fn integer_arithmetic_comparisons_and_explicit_wrapping_execute() {
    let cases = [
        (
            BinaryOp::Add,
            Constant::I64(1000),
            Constant::I64(24),
            "1024",
        ),
        (
            BinaryOp::Subtract,
            Constant::I32(-50),
            Constant::I32(25),
            "-75",
        ),
        (
            BinaryOp::Multiply,
            Constant::I64(4096),
            Constant::I64(1024),
            "4194304",
        ),
        (BinaryOp::Divide, Constant::I64(-7), Constant::I64(3), "-2"),
        (
            BinaryOp::Remainder,
            Constant::I64(-7),
            Constant::I64(3),
            "-1",
        ),
        (
            BinaryOp::AddWrapping,
            Constant::I32(i32::MAX),
            Constant::I32(1),
            "-2147483648",
        ),
        (
            BinaryOp::SubtractWrapping,
            Constant::I64(i64::MIN),
            Constant::I64(1),
            "9223372036854775807",
        ),
        (
            BinaryOp::MultiplyWrapping,
            Constant::I64(i64::MAX),
            Constant::I64(2),
            "-2",
        ),
        (BinaryOp::Less, Constant::I64(-3), Constant::I64(2), "1"),
        (BinaryOp::Equal, Constant::Unit, Constant::Unit, "1"),
        (
            BinaryOp::NotEqual,
            Constant::Bool(true),
            Constant::Bool(false),
            "1",
        ),
        (BinaryOp::BitAnd, Constant::I32(14), Constant::I32(11), "10"),
        (BinaryOp::BitOr, Constant::I32(8), Constant::I32(3), "11"),
        (BinaryOp::BitXor, Constant::I32(11), Constant::I32(3), "8"),
    ];
    for (operation, left, right, expected) in cases {
        returns(
            &checked_simple(binary_function(operation, left, right)),
            FunctionId(0),
            expected,
            Optimization::O2,
        );
    }
}

#[test]
fn checked_integer_failures_trap_in_native_code_before_printing() {
    let mut cases = Vec::new();
    for width in [32, 64] {
        let (minimum, maximum, zero, one, two, minus_one) = if width == 32 {
            (
                Constant::I32(i32::MIN),
                Constant::I32(i32::MAX),
                Constant::I32(0),
                Constant::I32(1),
                Constant::I32(2),
                Constant::I32(-1),
            )
        } else {
            (
                Constant::I64(i64::MIN),
                Constant::I64(i64::MAX),
                Constant::I64(0),
                Constant::I64(1),
                Constant::I64(2),
                Constant::I64(-1),
            )
        };
        cases.push(binary_function(BinaryOp::Add, maximum.clone(), one.clone()));
        cases.push(binary_function(
            BinaryOp::Subtract,
            minimum.clone(),
            one.clone(),
        ));
        cases.push(binary_function(BinaryOp::Multiply, maximum, two));
        cases.push(binary_function(BinaryOp::Divide, one.clone(), zero.clone()));
        cases.push(binary_function(
            BinaryOp::Divide,
            minimum.clone(),
            minus_one.clone(),
        ));
        cases.push(binary_function(BinaryOp::Remainder, one, zero));
        cases.push(binary_function(
            BinaryOp::Remainder,
            minimum.clone(),
            minus_one,
        ));
        let ty = minimum.ty();
        cases.push(function(
            0,
            "check",
            ty.clone(),
            vec![ty.clone(), ty],
            vec![block(
                0,
                vec![
                    constant(0, minimum),
                    instruction(
                        Some(1),
                        InstructionKind::Unary {
                            op: UnaryOp::Negate,
                            operand: ValueId(0),
                        },
                    ),
                ],
                Terminator::Return(Some(ValueId(1))),
            )],
        ));
    }
    for fixture in cases {
        let verified = checked_simple(fixture);
        for optimization in [Optimization::O0, Optimization::O2] {
            let output = native(
                &verified,
                EntryPoint::ReturnValue(FunctionId(0)),
                optimization,
            );
            assert!(
                !output.status.success(),
                "checked failure incorrectly succeeded"
            );
            #[cfg(unix)]
            {
                use std::os::unix::process::ExitStatusExt;
                assert!(
                    output.status.signal().is_some(),
                    "expected the LLVM trap signal, got {}",
                    output.status
                );
            }
            assert!(
                output.stdout.is_empty(),
                "failure returned a fabricated value"
            );
        }
    }
}

#[test]
fn loop_and_branch_execute_mutable_local_dataflow() {
    let mut sum = function(
        0,
        "check",
        Type::I64,
        vec![
            Type::I64,
            Type::I64,
            Type::I64,
            Type::Bool,
            Type::I64,
            Type::I64,
            Type::I64,
            Type::I64,
            Type::I64,
            Type::I64,
        ],
        vec![
            block(
                0,
                vec![constant(0, Constant::I64(0)), store(0, 0), store(1, 0)],
                Terminator::Jump(BlockId(1)),
            ),
            block(
                1,
                vec![
                    load(1, 0),
                    constant(2, Constant::I64(10)),
                    binary(3, BinaryOp::Less, 1, 2),
                ],
                Terminator::Branch {
                    condition: ValueId(3),
                    then_block: BlockId(2),
                    else_block: BlockId(3),
                },
            ),
            block(
                2,
                vec![
                    load(4, 1),
                    load(5, 0),
                    constant(6, Constant::I64(1)),
                    binary(7, BinaryOp::Add, 5, 6),
                    binary(8, BinaryOp::Add, 4, 7),
                    store(0, 7),
                    store(1, 8),
                ],
                Terminator::Jump(BlockId(1)),
            ),
            block(3, vec![load(9, 1)], Terminator::Return(Some(ValueId(9)))),
        ],
    );
    sum.locals = vec![local(0, Type::I64, true), local(1, Type::I64, true)];
    let initialized = vec![0, 1];
    let verified = checked(
        vec![sum],
        vec![vec![
            (vec![], initialized.clone()),
            (initialized.clone(), initialized.clone()),
            (initialized.clone(), initialized.clone()),
            (initialized.clone(), initialized),
        ]],
    );
    for optimization in [Optimization::O0, Optimization::O2] {
        returns(&verified, FunctionId(0), "55", optimization);
    }
}

#[test]
fn recursion_and_direct_calls_use_identity_symbols_not_source_names() {
    let mut factorial = function(
        0,
        "factorial",
        Type::I64,
        vec![
            Type::I64,
            Type::I64,
            Type::Bool,
            Type::I64,
            Type::I64,
            Type::I64,
            Type::I64,
            Type::I64,
            Type::I64,
        ],
        vec![
            block(
                0,
                vec![
                    load(0, 0),
                    constant(1, Constant::I64(1)),
                    binary(2, BinaryOp::LessEqual, 0, 1),
                ],
                Terminator::Branch {
                    condition: ValueId(2),
                    then_block: BlockId(1),
                    else_block: BlockId(2),
                },
            ),
            block(
                1,
                vec![constant(3, Constant::I64(1))],
                Terminator::Return(Some(ValueId(3))),
            ),
            block(
                2,
                vec![
                    load(4, 0),
                    constant(5, Constant::I64(1)),
                    binary(6, BinaryOp::Subtract, 4, 5),
                    instruction(
                        Some(7),
                        InstructionKind::Call {
                            function: FunctionId(0),
                            arguments: vec![ValueId(6)],
                        },
                    ),
                    binary(8, BinaryOp::Multiply, 4, 7),
                ],
                Terminator::Return(Some(ValueId(8))),
            ),
        ],
    );
    factorial.locals = vec![local(0, Type::I64, false)];
    factorial.parameters = vec![LocalId(0)];
    let check = function(
        1,
        "a source name\" }\ndefine i32 @injected()",
        Type::I64,
        vec![Type::I64, Type::I64],
        vec![block(
            0,
            vec![
                constant(0, Constant::I64(12)),
                instruction(
                    Some(1),
                    InstructionKind::Call {
                        function: FunctionId(0),
                        arguments: vec![ValueId(0)],
                    },
                ),
            ],
            Terminator::Return(Some(ValueId(1))),
        )],
    );
    let verified = checked(
        vec![factorial, check],
        vec![vec![(vec![0], vec![0]); 3], vec![(vec![], vec![])]],
    );
    let llvm = emit(&verified, &lir(&verified)).unwrap();
    assert!(llvm.contains("@zil.fn.0"));
    assert!(!llvm.contains("injected"));
    assert!(!llvm.contains("noalias"));
    assert!(!llvm.contains("dereferenceable"));
    for optimization in [Optimization::O0, Optimization::O2] {
        returns(&verified, FunctionId(1), "479001600", optimization);
    }
}

#[test]
fn nonzero_cfg_entry_is_selected_by_the_native_prologue() {
    let mut check = function(
        0,
        "check",
        Type::I64,
        vec![Type::I64, Type::I64],
        vec![
            block(
                0,
                vec![constant(0, Constant::I64(7))],
                Terminator::Return(Some(ValueId(0))),
            ),
            block(
                1,
                vec![constant(1, Constant::I64(1024))],
                Terminator::Return(Some(ValueId(1))),
            ),
        ],
    );
    check.entry = BlockId(1);
    returns(
        &checked_simple(check),
        FunctionId(0),
        "1024",
        Optimization::O0,
    );
}

#[test]
fn unit_parameters_loads_stores_calls_and_returns_are_erased_after_verification() {
    let mut callee = function(
        0,
        "unit_call",
        Type::Unit,
        vec![Type::Unit, Type::Bool, Type::Bool],
        vec![block(
            0,
            vec![
                load(0, 0),
                store(0, 0),
                load(1, 1),
                instruction(
                    Some(2),
                    InstructionKind::Unary {
                        op: UnaryOp::Not,
                        operand: ValueId(1),
                    },
                ),
            ],
            Terminator::Return(Some(ValueId(0))),
        )],
    );
    callee.parameters = vec![LocalId(0), LocalId(1)];
    callee.locals = vec![local(0, Type::Unit, true), local(1, Type::Bool, false)];
    let check = function(
        1,
        "check",
        Type::I64,
        vec![Type::Unit, Type::Bool, Type::I64],
        vec![block(
            0,
            vec![
                constant(0, Constant::Unit),
                constant(1, Constant::Bool(true)),
                instruction(
                    None,
                    InstructionKind::Call {
                        function: FunctionId(0),
                        arguments: vec![ValueId(0), ValueId(1)],
                    },
                ),
                constant(2, Constant::I64(1024)),
            ],
            Terminator::Return(Some(ValueId(2))),
        )],
    );
    let verified = checked(
        vec![callee, check],
        vec![vec![(vec![0, 1], vec![0, 1])], vec![(vec![], vec![])]],
    );
    let llvm = emit(&verified, &lir(&verified)).unwrap();
    assert!(llvm.contains("define void @zil.fn.0(i1 %arg.1)"));
    assert!(!llvm.contains("alloca void"));
    returns(&verified, FunctionId(1), "1024", Optimization::O0);
}

#[test]
fn tampered_lir_is_rejected_before_invoking_tools_or_creating_outputs() {
    let verified = checked_simple(returning(Constant::I64(1024)));
    let original = lir(&verified);
    let mut mutants = Vec::new();
    let mut changed = original.clone();
    changed.verified_fingerprint = Fingerprint::ZERO;
    mutants.push(changed);
    let mut changed = original.clone();
    changed.module_id = ModuleId(999);
    mutants.push(changed);
    let mut changed = original.clone();
    changed.schema_version += 1;
    mutants.push(changed);
    let mut changed = original.clone();
    changed.target.pointer_width = 32;
    mutants.push(changed);
    let mut changed = original.clone();
    changed.layouts[3].alignment = 4;
    mutants.push(changed);
    let mut changed = original.clone();
    changed.functions[0].blocks[0].instructions[0].kind =
        InstructionKind::Constant(Constant::I64(99));
    mutants.push(changed);
    let mut changed = original;
    changed.functions[0].name = "main".into();
    mutants.push(changed);
    for mutant in mutants {
        let directory = TestDirectory::new();
        let mut options = BuildOptions::new(directory.0.join("must-not-exist"));
        options.clang = directory.0.join("no-such-compiler");
        options.emit.entry = EntryPoint::ReturnValue(FunctionId(0));
        let errors = build(&verified, &mutant, &options).unwrap_err();
        let view_errors =
            build_view_with_cancellation(&verified, &mutant.as_view(), &options, &|| false)
                .unwrap_err();
        assert_eq!(errors, view_errors);
        assert_eq!(errors[0].code, "E-CODEGEN-UNBOUND-LIR");
        assert_eq!(errors[0].category, ResultCategory::SafeReject);
        assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 0);
    }
}

#[test]
fn bytes083_native_unsigned_result_and_untrusted_byte_layout_are_independent() {
    let verified = checked_simple(returning(Constant::U8(255)));
    for optimization in [Optimization::O0, Optimization::O2] {
        returns(&verified, FunctionId(0), "255", optimization);
    }
    let mut changed = lir(&verified);
    changed
        .layouts
        .iter_mut()
        .find(|layout| layout.ty == Type::U8)
        .unwrap()
        .alignment = 2;
    assert_eq!(
        emit(&verified, &changed).unwrap_err()[0].code,
        "E-CODEGEN-UNBOUND-LIR"
    );
}

#[test]
fn missing_tool_and_invalid_entry_fail_without_a_native_artifact() {
    let verified = checked_simple(returning(Constant::I64(1024)));
    let directory = TestDirectory::new();
    let mut options = BuildOptions::new(directory.0.join("must-not-exist"));
    options.clang = directory.0.join("no-such-compiler");
    options.emit.entry = EntryPoint::ReturnValue(FunctionId(0));
    let errors = build(&verified, &lir(&verified), &options).unwrap_err();
    assert_eq!(errors[0].category, ResultCategory::InfraError);
    assert!(!options.output.exists());
    assert_eq!(
        fs::read_dir(&directory.0).unwrap().count(),
        0,
        "temporary IR was not cleaned up"
    );
    options.emit.entry = EntryPoint::Main;
    let errors = build(&verified, &lir(&verified), &options).unwrap_err();
    assert_eq!(errors[0].code, "E-CODEGEN-ENTRY");
    options.emit.entry = EntryPoint::None;
    let errors = build(&verified, &lir(&verified), &options).unwrap_err();
    assert_eq!(errors[0].code, "E-LINK-ENTRY");
    options.emit.entry = EntryPoint::ReturnValue(FunctionId(77));
    let errors = build(&verified, &lir(&verified), &options).unwrap_err();
    assert_eq!(errors[0].code, "E-CODEGEN-ENTRY");
    assert!(!options.output.exists());
}

fn make_closure(result: u32, closure: u32, captures: &[u32]) -> Instruction {
    instruction(
        Some(result),
        InstructionKind::MakeClosure {
            closure: ClosureId(closure),
            captures: captures.iter().copied().map(ValueId).collect(),
        },
    )
}
fn call_closure(result: Option<u32>, callee: u32, arguments: &[u32]) -> Instruction {
    instruction(
        result,
        InstructionKind::CallClosure {
            callee: ValueId(callee),
            arguments: arguments.iter().copied().map(ValueId).collect(),
        },
    )
}
fn read_capture(result: u32, index: u32) -> Instruction {
    instruction(Some(result), InstructionKind::ReadCapture { index })
}

#[test]
fn runtime_closure_environments_are_independent_copies_and_repeatable() {
    let check = function(
        0,
        "check",
        Type::I64,
        vec![Type::I64; 5],
        vec![block(
            0,
            vec![
                constant(0, Constant::I64(40)),
                instruction(
                    Some(1),
                    InstructionKind::Call {
                        function: FunctionId(1),
                        arguments: vec![ValueId(0)],
                    },
                ),
                constant(2, Constant::I64(100)),
                instruction(
                    Some(3),
                    InstructionKind::Call {
                        function: FunctionId(1),
                        arguments: vec![ValueId(2)],
                    },
                ),
                binary(4, BinaryOp::Add, 1, 3),
            ],
            Terminator::Return(Some(ValueId(4))),
        )],
    );
    let closure_type = Type::Closure(ClosureId(0));
    let mut calculate = function(
        1,
        "calculate",
        Type::I64,
        vec![
            Type::I64,
            closure_type.clone(),
            closure_type.clone(),
            Type::I64,
            closure_type.clone(),
            closure_type.clone(),
            Type::I64,
            Type::I64,
            Type::I64,
            closure_type.clone(),
            Type::I64,
            Type::I64,
            Type::I64,
        ],
        vec![block(
            0,
            vec![
                load(0, 0),
                make_closure(1, 0, &[0]),
                store(1, 1),
                load(2, 1),
                store(2, 2),
                constant(3, Constant::I64(1000)),
                store(0, 3),
                make_closure(4, 0, &[3]),
                store(1, 4),
                load(5, 2),
                constant(6, Constant::I64(2)),
                call_closure(Some(7), 5, &[6]),
                call_closure(Some(8), 5, &[6]),
                load(9, 1),
                call_closure(Some(10), 9, &[6]),
                binary(11, BinaryOp::Add, 7, 8),
                binary(12, BinaryOp::Add, 11, 10),
            ],
            Terminator::Return(Some(ValueId(12))),
        )],
    );
    calculate.parameters = vec![LocalId(0)];
    calculate.locals = vec![
        local(0, Type::I64, true),
        local(1, closure_type.clone(), true),
        local(2, closure_type, false),
    ];
    let mut closure = function(
        2,
        "$closure0",
        Type::I64,
        vec![Type::I64; 3],
        vec![block(
            0,
            vec![
                read_capture(0, 0),
                load(1, 0),
                binary(2, BinaryOp::Add, 0, 1),
            ],
            Terminator::Return(Some(ValueId(2))),
        )],
    );
    closure.environment = Some(ClosureId(0));
    closure.parameters = vec![LocalId(0)];
    closure.locals = vec![local(0, Type::I64, false)];
    let verified = checked_with_closures(
        vec![check, calculate, closure],
        vec![ClosureDefinition {
            lending_captures: Vec::new(),
            id: ClosureId(0),
            function: FunctionId(2),
            captures: vec![Type::I64],
            parameters: vec![Type::I64],
            result: Type::I64,
            kind: CallableKind::Fn,
        }],
        vec![
            vec![(vec![], vec![])],
            vec![(vec![0], vec![0, 1, 2])],
            vec![(vec![0], vec![0])],
        ],
    );
    let llvm = emit(&verified, &lir(&verified)).unwrap();
    assert!(llvm.contains("%zil_env_0 = type { i64 }"));
    assert!(llvm.contains("define i64 @zil.fn.2(%zil_env_0 %env, i64 %arg.0)"));
    assert!(llvm.contains("alloca %zil_env_0, align 8"));
    assert!(llvm.contains("insertvalue %zil_env_0"));
    assert!(llvm.contains("extractvalue %zil_env_0 %env, 0"));
    for optimization in [Optimization::O0, Optimization::O2] {
        // 2 * 40 + 1006 plus 2 * 100 + 1006. Replacing either the captured
        // variable or one closure local must not change the copied environment.
        returns(&verified, FunctionId(0), "2292", optimization);
    }
}

#[test]
fn empty_environments_and_unit_mixed_fields_use_one_consistent_native_abi() {
    let mut check = function(
        0,
        "check",
        Type::I64,
        vec![
            Type::Unit,
            Type::Bool,
            Type::I32,
            Type::I64,
            Type::Closure(ClosureId(0)),
            Type::Closure(ClosureId(0)),
            Type::I64,
            Type::I64,
            Type::Closure(ClosureId(1)),
            Type::Closure(ClosureId(1)),
            Type::Closure(ClosureId(2)),
            Type::Closure(ClosureId(2)),
            Type::I64,
        ],
        vec![block(
            0,
            vec![
                constant(0, Constant::Unit),
                constant(1, Constant::Bool(true)),
                constant(2, Constant::I32(7)),
                constant(3, Constant::I64(40)),
                make_closure(4, 0, &[0, 1, 2, 3, 0]),
                store(0, 4),
                load(5, 0),
                constant(6, Constant::I64(2)),
                call_closure(Some(7), 5, &[0, 2, 0, 6, 1, 0]),
                make_closure(8, 1, &[]),
                store(1, 8),
                load(9, 1),
                call_closure(None, 9, &[0]),
                make_closure(10, 2, &[]),
                store(2, 10),
                load(11, 2),
                call_closure(Some(12), 11, &[0, 7]),
            ],
            Terminator::Return(Some(ValueId(12))),
        )],
    );
    check.locals = (0..3)
        .map(|id| local(id, Type::Closure(ClosureId(id)), false))
        .collect();
    let mut mixed = function(
        1,
        "$closure0",
        Type::I64,
        vec![
            Type::Unit,
            Type::Bool,
            Type::I32,
            Type::I64,
            Type::Unit,
            Type::I32,
            Type::Bool,
            Type::I64,
            Type::I64,
            Type::Bool,
            Type::Bool,
            Type::Bool,
            Type::I64,
        ],
        vec![
            block(
                0,
                vec![
                    read_capture(0, 0),
                    read_capture(1, 1),
                    read_capture(2, 2),
                    read_capture(3, 3),
                    read_capture(4, 4),
                    load(5, 1),
                    binary(6, BinaryOp::Equal, 2, 5),
                    load(7, 3),
                    binary(8, BinaryOp::Add, 3, 7),
                    load(9, 4),
                    binary(10, BinaryOp::And, 1, 6),
                    binary(11, BinaryOp::And, 10, 9),
                ],
                Terminator::Branch {
                    condition: ValueId(11),
                    then_block: BlockId(1),
                    else_block: BlockId(2),
                },
            ),
            block(1, vec![], Terminator::Return(Some(ValueId(8)))),
            block(
                2,
                vec![constant(12, Constant::I64(-1))],
                Terminator::Return(Some(ValueId(12))),
            ),
        ],
    );
    mixed.environment = Some(ClosureId(0));
    let mixed_parameters = vec![
        Type::Unit,
        Type::I32,
        Type::Unit,
        Type::I64,
        Type::Bool,
        Type::Unit,
    ];
    mixed.locals = mixed_parameters
        .iter()
        .enumerate()
        .map(|(id, ty)| local(id as u32, ty.clone(), false))
        .collect();
    mixed.parameters = (0..6).map(LocalId).collect();
    let mut unit = function(
        2,
        "$closure1",
        Type::Unit,
        vec![Type::Unit],
        vec![block(0, vec![load(0, 0)], Terminator::Return(None))],
    );
    unit.environment = Some(ClosureId(1));
    unit.locals = vec![local(0, Type::Unit, false)];
    unit.parameters = vec![LocalId(0)];
    let mut empty = function(
        3,
        "$closure2",
        Type::I64,
        vec![Type::I64],
        vec![block(
            0,
            vec![load(0, 1)],
            Terminator::Return(Some(ValueId(0))),
        )],
    );
    empty.environment = Some(ClosureId(2));
    empty.locals = vec![local(0, Type::Unit, false), local(1, Type::I64, false)];
    empty.parameters = vec![LocalId(0), LocalId(1)];
    let mixed_initialized = (0..6).collect::<Vec<_>>();
    let verified = checked_with_closures(
        vec![check, mixed, unit, empty],
        vec![
            ClosureDefinition {
                lending_captures: Vec::new(),
                id: ClosureId(0),
                function: FunctionId(1),
                captures: vec![Type::Unit, Type::Bool, Type::I32, Type::I64, Type::Unit],
                parameters: mixed_parameters,
                result: Type::I64,
                kind: CallableKind::Fn,
            },
            ClosureDefinition {
                lending_captures: Vec::new(),
                id: ClosureId(1),
                function: FunctionId(2),
                captures: vec![],
                parameters: vec![Type::Unit],
                result: Type::Unit,
                kind: CallableKind::Fn,
            },
            ClosureDefinition {
                lending_captures: Vec::new(),
                id: ClosureId(2),
                function: FunctionId(3),
                captures: vec![],
                parameters: vec![Type::Unit, Type::I64],
                result: Type::I64,
                kind: CallableKind::Fn,
            },
        ],
        vec![
            vec![(vec![], vec![0, 1, 2])],
            vec![(mixed_initialized.clone(), mixed_initialized); 3],
            vec![(vec![0], vec![0])],
            vec![(vec![0, 1], vec![0, 1])],
        ],
    );
    let llvm = emit(&verified, &lir(&verified)).unwrap();
    assert!(llvm.contains("%zil_env_0 = type { i8, i1, i32, i64, i8 }"));
    assert!(llvm.contains("%zil_env_1 = type {  }"));
    assert!(llvm.contains("define void @zil.fn.2(%zil_env_1 %env)"));
    assert!(llvm.contains("define i64 @zil.fn.3(%zil_env_2 %env, i64 %arg.1)"));
    assert!(llvm.contains("freeze %zil_env_1 zeroinitializer"));
    assert!(!llvm.contains("alloca void"));
    assert!(!llvm.contains("insertvalue %zil_env_0 zeroinitializer, void"));
    for optimization in [Optimization::O0, Optimization::O2] {
        returns(&verified, FunctionId(0), "42", optimization);
    }
}

fn empty_closure_fixture(environment_name: &str) -> VerifiedModule {
    let check = function(
        0,
        "check",
        Type::I64,
        vec![Type::Closure(ClosureId(0)), Type::I64],
        vec![block(
            0,
            vec![make_closure(0, 0, &[]), call_closure(Some(1), 0, &[])],
            Terminator::Return(Some(ValueId(1))),
        )],
    );
    let mut closure = returning(Constant::I64(42));
    closure.id = FunctionId(1);
    closure.name = environment_name.into();
    closure.environment = Some(ClosureId(0));
    checked_with_closures(
        vec![check, closure],
        vec![ClosureDefinition {
            lending_captures: Vec::new(),
            id: ClosureId(0),
            function: FunctionId(1),
            captures: vec![],
            parameters: vec![],
            result: Type::I64,
            kind: CallableKind::Fn,
        }],
        vec![vec![(vec![], vec![])]; 2],
    )
}

#[test]
fn native_entry_never_enters_an_environment_function_even_when_empty() {
    let verified = empty_closure_fixture("main");
    for entry in [EntryPoint::Main, EntryPoint::ReturnValue(FunctionId(1))] {
        let directory = TestDirectory::new();
        let mut options = BuildOptions::new(directory.0.join("must-not-exist"));
        options.emit.entry = entry;
        options.clang = directory.0.join("no-such-compiler");
        let errors = build(&verified, &lir(&verified), &options).unwrap_err();
        assert_eq!(errors[0].code, "E-CODEGEN-ENTRY");
        assert!(errors[0].message.contains("requires its environment"));
        assert_eq!(errors[0].category, ResultCategory::SafeReject);
        assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 0);
    }
}

#[test]
fn changing_any_lir_closure_claim_is_rejected_before_tools_or_output() {
    let verified = empty_closure_fixture("$closure0");
    let original = lir(&verified);
    let mut mutants = Vec::new();
    let mut changed = original.clone();
    changed.closures[0].id = ClosureId(1);
    mutants.push(changed);
    let mut changed = original.clone();
    changed.closures[0].function = FunctionId(0);
    mutants.push(changed);
    let mut changed = original.clone();
    changed.closures[0].captures.push(Type::I64);
    mutants.push(changed);
    let mut changed = original.clone();
    changed.closures[0].parameters.push(Type::I64);
    mutants.push(changed);
    let mut changed = original.clone();
    changed.closures[0].result = Type::I32;
    mutants.push(changed);
    let mut changed = original.clone();
    changed.closures[0].kind = CallableKind::FnMut;
    mutants.push(changed);
    let mut changed = original.clone();
    changed.closures.push(changed.closures[0].clone());
    mutants.push(changed);
    let mut changed = original;
    changed.closures.clear();
    mutants.push(changed);
    for mutant in mutants {
        let directory = TestDirectory::new();
        let mut options = BuildOptions::new(directory.0.join("must-not-exist"));
        options.emit.entry = EntryPoint::ReturnValue(FunctionId(0));
        options.clang = directory.0.join("no-such-compiler");
        let errors = build(&verified, &mutant, &options).unwrap_err();
        assert_eq!(errors[0].code, "E-CODEGEN-UNBOUND-LIR");
        assert_eq!(errors[0].category, ResultCategory::SafeReject);
        assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 0);
    }
}

#[test]
fn borrowed_lir_aliases_bind_length_and_the_current_capability() {
    let verified = checked_simple(returning(Constant::I64(1024)));
    let original = lir(&verified);
    let mut view = original.as_view();
    view.functions = verified.module().functions.as_slice();
    let options = EmitOptions::default();
    assert_eq!(
        crate::emit::lower_view_with_options(&verified, &view, &options).unwrap(),
        crate::emit::lower_with_options(&verified, &original, &options).unwrap()
    );
    view.functions = &verified.module().functions[..0];
    assert_eq!(view.functions.as_ptr(), verified.module().functions.as_ptr());
    assert_eq!(
        crate::emit::lower_view_with_options(&verified, &view, &options).unwrap_err()[0].code,
        "E-CODEGEN-UNBOUND-LIR"
    );
    let other = checked_simple(returning(Constant::I64(99)));
    view.functions = verified.module().functions.as_slice();
    view.verified_fingerprint = other.fingerprint();
    assert_eq!(
        crate::emit::lower_view_with_options(&other, &view, &options).unwrap_err()[0].code,
        "E-CODEGEN-UNBOUND-LIR"
    );
}
