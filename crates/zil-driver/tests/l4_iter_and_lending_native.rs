//! End-to-end dual-backend native execution, std.iter / LendingIterator closure,
//! safety rejection, and Raw/Wire independent `zil-check` verifier tests for
//! Batch L4-2 (G9.3).

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use zil_base::{FileId, Phase, ResultCategory, Span};
use zil_check::{CheckBudget, CheckErrorKind, Violation, ViolationKind, check_module};
use zil_driver::{
    CodegenOptions, CompilationSession, CompileOptions, NativeBackend, Optimization, Stage,
};
use zil_ir::body::{
    BinOp, Block, BlockId, Body, Callee, Certificate, ConstValue, FnId, FnRole, FnSig, Function,
    FunctionCertificate, Instruction, InstructionKind, IntTy, IntrinsicOp, Local, LocalDecl,
    Module, Mutability, OpaqueTy, Operand, ParamEnv, Place, Projection, Region, ResumeMode, Rvalue,
    Terminator, TyKind, TyTable, decode_module, encode_module,
};
use zil_solve::{Budget, SolveError};

fn sp() -> Span {
    Span::new(FileId(0), 0, 0)
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .to_path_buf()
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("zil-l4-2-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn dual_configs() -> Vec<(&'static str, NativeBackend, Optimization)> {
    let mut configs = vec![
        ("clang-o0", NativeBackend::Clang, Optimization::O0),
        ("clang-o2", NativeBackend::Clang, Optimization::O2),
    ];
    if cfg!(feature = "llvm-api") {
        configs.push(("llvmapi-o0", NativeBackend::LlvmApi, Optimization::O0));
        configs.push(("llvmapi-o2", NativeBackend::LlvmApi, Optimization::O2));
    }
    configs
}

fn build_and_run_dual(dir: &Path, entry: &Path) -> (i32, String) {
    let mut expected: Option<(i32, String)> = None;
    for (label, backend, opt) in dual_configs() {
        let exe = dir.join(format!("bin-{label}"));
        let mut session = CompilationSession::new();
        let opts = CompileOptions {
            stop_after: Stage::Build,
            optimization: opt,
            backend,
            output: Some(exe.clone()),
            codegen: CodegenOptions { units: 1, jobs: 1 },
            ..CompileOptions::default()
        };
        let report = session.compile_path(entry, &opts);
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "build failed for {} ({label}): {:?}",
            entry.display(),
            report.diagnostics
        );
        assert_eq!(report.verification_runs, 1);

        let out = Command::new(&exe)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .unwrap_or_else(|e| panic!("failed to run {}: {e}", exe.display()));
        assert!(
            out.stderr.is_empty(),
            "unexpected stderr ({label}) for {}: {}",
            entry.display(),
            String::from_utf8_lossy(&out.stderr)
        );
        let code = out.status.code().unwrap_or(-1);
        let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        if let Some((prev_code, ref prev_stdout)) = expected {
            assert_eq!(code, prev_code, "exit mismatch on {label}");
            assert_eq!(&stdout, prev_stdout, "stdout mismatch on {label}");
        } else {
            expected = Some((code, stdout));
        }
    }
    expected.expect("at least one backend ran")
}

#[test]
fn l4_2_iterator_refill_and_string_search_native() {
    let root = repo_root().join("tests/language-readiness/terminal/library-and-ui");
    let cases = [
        ("iterator", root.join("iterator.zil")),
        (
            "callable_store_field_refill",
            root.join("growing-hash-collections/callable-store-field-refill.zil"),
        ),
        ("string_search", root.join("string_search.zil")),
        ("store_element_field", root.join("store_element_field.zil")),
    ];

    for (name, path) in cases {
        let dir = temp_dir(name);
        let (code, _stdout) = build_and_run_dual(&dir, &path);
        assert_eq!(code, 0, "expected exit 0 for {}", path.display());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[test]
fn l4_2_owning_iter_pipeline_and_lending_iterator_native() {
    let root = repo_root().join("tests/language-readiness/iterator-adapters");
    let cases = [
        ("owning_pipeline", root.join("owning-pipeline.zil")),
        ("owned_fold", root.join("owned-fold.zil")),
        ("borrowed_consumer", root.join("borrowed-consumer.zil")),
    ];

    for (name, path) in cases {
        let dir = temp_dir(name);
        let (code, _stdout) = build_and_run_dual(&dir, &path);
        assert_eq!(code, 0, "expected exit 0 for {}", path.display());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[test]
fn l4_2_safety_negative_store_borrow_and_iterator_capture_reset() {
    let root = repo_root().join("tests/language-readiness/terminal/library-and-ui");

    // 1. store_element_field_conflict.zil must be strictly rejected at Phase::Analyze
    //    with E-SAFETY-SOLVE and LoanConflict.
    {
        let entry = root.join("store_element_field_conflict.zil");
        let mut session = CompilationSession::new();
        let report = session.compile_path(
            &entry,
            &CompileOptions {
                stop_after: Stage::Verify,
                ..CompileOptions::default()
            },
        );
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "expected SafeReject for {}: {:?}",
            entry.display(),
            report.diagnostics
        );
        assert!(
            report.diagnostics.iter().any(|d| d.phase == Phase::Analyze
                && d.code == "E-SAFETY-SOLVE"
                && d.message.contains("LoanConflict")),
            "expected Phase::Analyze E-SAFETY-SOLVE LoanConflict for {}, got: {:?}",
            entry.display(),
            report.diagnostics
        );
    }

    // 2. iterator_capture_reset_invalid.zil must be strictly rejected at Phase::Analyze
    //    with E-SAFETY-SOLVE.
    {
        let entry = root.join("iterator_capture_reset_invalid.zil");
        let mut session = CompilationSession::new();
        let report = session.compile_path(
            &entry,
            &CompileOptions {
                stop_after: Stage::Verify,
                ..CompileOptions::default()
            },
        );
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "expected SafeReject for {}: {:?}",
            entry.display(),
            report.diagnostics
        );
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.phase == Phase::Analyze && d.code == "E-SAFETY-SOLVE"),
            "expected Phase::Analyze E-SAFETY-SOLVE for {}, got: {:?}",
            entry.display(),
            report.diagnostics
        );
    }
}

/// Builds a Raw/Wire module with two genuinely independent locally-created
/// `Store<i64>` owners (`StoreNew`), each described by a single-definition
/// `Describe` local (`d0 = Describe(s0)`, `d1 = Describe(s1)`), borrowed
/// concurrently with `Shared` + `Exclusive` `StoreBorrow`. Because both owners
/// are distinct local owning `Store<i64>` values (`proven_unique_owner == true`),
/// they are mathematically apart and MUST be accepted by both Solver and `zil-check`.
fn build_disjoint_owned_stores_module(exclusive_first: bool) -> Module {
    let mut tys = TyTable::new();
    let i64_ty = tys.intern(TyKind::Int(IntTy::I64));
    let store_opaque = tys.intern(TyKind::Opaque(OpaqueTy::OwnedStore { element: i64_ty }));
    let store_managed = tys.intern(TyKind::Managed(store_opaque));
    let store_desc = tys.intern(TyKind::Desc(store_opaque));
    let shared_ref = tys.intern(TyKind::Ref {
        region: Region::Static,
        mutability: Mutability::Shared,
        pointee: i64_ty,
    });
    let excl_ref = tys.intern(TyKind::Ref {
        region: Region::Static,
        mutability: Mutability::Exclusive,
        pointee: i64_ty,
    });

    let (first_mut, second_mut, first_ty, second_ty) = if exclusive_first {
        (
            Mutability::Exclusive,
            Mutability::Shared,
            excl_ref,
            shared_ref,
        )
    } else {
        (
            Mutability::Shared,
            Mutability::Exclusive,
            shared_ref,
            excl_ref,
        )
    };

    // Locals:
    //   Local(0): s0: Managed(OwnedStore<i64>) (created by StoreNew)
    //   Local(1): s1: Managed(OwnedStore<i64>) (created by StoreNew)
    //   Local(2): d0: Desc(OwnedStore<i64>) = Describe(s0)
    //   Local(3): d1: Desc(OwnedStore<i64>) = Describe(s1)
    //   Local(4): r0: &i64 or &mut i64
    //   Local(5): r1: &mut i64 or &i64
    //   Local(6): v0: i64
    //   Local(7): v1: i64
    //   Local(8): sum: i64
    let body = Body {
        param_count: 0,
        param_env: ParamEnv::default(),
        closure: None,
        captures: Vec::new(),
        result: i64_ty,
        locals: vec![
            LocalDecl {
                ty: store_managed,
                name: Some("s0".into()),
            },
            LocalDecl {
                ty: store_managed,
                name: Some("s1".into()),
            },
            LocalDecl {
                ty: store_desc,
                name: Some("d0".into()),
            },
            LocalDecl {
                ty: store_desc,
                name: Some("d1".into()),
            },
            LocalDecl {
                ty: first_ty,
                name: Some("r0".into()),
            },
            LocalDecl {
                ty: second_ty,
                name: Some("r1".into()),
            },
            LocalDecl {
                ty: i64_ty,
                name: Some("v0".into()),
            },
            LocalDecl {
                ty: i64_ty,
                name: Some("v1".into()),
            },
            LocalDecl {
                ty: i64_ty,
                name: Some("sum".into()),
            },
        ],
        drop_flags: Vec::new(),
        blocks: vec![
            Block {
                instructions: vec![Instruction {
                    kind: InstructionKind::StorageLive(Local(0)),
                    span: sp(),
                }],
                terminator: Terminator::Call {
                    callee: Callee::Intrinsic(IntrinsicOp::StoreNew),
                    args: Vec::new(),
                    dest: Place::local(Local(0)),
                    target: BlockId(1),
                    cancel: None,
                },
                terminator_span: sp(),
            },
            Block {
                instructions: vec![Instruction {
                    kind: InstructionKind::StorageLive(Local(1)),
                    span: sp(),
                }],
                terminator: Terminator::Call {
                    callee: Callee::Intrinsic(IntrinsicOp::StoreNew),
                    args: Vec::new(),
                    dest: Place::local(Local(1)),
                    target: BlockId(2),
                    cancel: None,
                },
                terminator_span: sp(),
            },
            Block {
                instructions: vec![
                    Instruction {
                        kind: InstructionKind::StorageLive(Local(2)),
                        span: sp(),
                    },
                    Instruction {
                        kind: InstructionKind::Assign {
                            dest: Place::local(Local(2)),
                            value: Rvalue::Describe {
                                place: Place::local(Local(0)),
                            },
                        },
                        span: sp(),
                    },
                    Instruction {
                        kind: InstructionKind::StorageLive(Local(3)),
                        span: sp(),
                    },
                    Instruction {
                        kind: InstructionKind::Assign {
                            dest: Place::local(Local(3)),
                            value: Rvalue::Describe {
                                place: Place::local(Local(1)),
                            },
                        },
                        span: sp(),
                    },
                    Instruction {
                        kind: InstructionKind::StorageLive(Local(4)),
                        span: sp(),
                    },
                ],
                terminator: Terminator::Call {
                    callee: Callee::Intrinsic(IntrinsicOp::StoreBorrow {
                        mutability: first_mut,
                    }),
                    args: vec![
                        Operand::Copy(Place::local(Local(2))),
                        Operand::Const(ConstValue::Int {
                            ty: IntTy::I64,
                            bits: 0,
                        }),
                    ],
                    dest: Place::local(Local(4)),
                    target: BlockId(3),
                    cancel: None,
                },
                terminator_span: sp(),
            },
            Block {
                instructions: vec![Instruction {
                    kind: InstructionKind::StorageLive(Local(5)),
                    span: sp(),
                }],
                terminator: Terminator::Call {
                    callee: Callee::Intrinsic(IntrinsicOp::StoreBorrow {
                        mutability: second_mut,
                    }),
                    args: vec![
                        Operand::Copy(Place::local(Local(3))),
                        Operand::Const(ConstValue::Int {
                            ty: IntTy::I64,
                            bits: 0,
                        }),
                    ],
                    dest: Place::local(Local(5)),
                    target: BlockId(4),
                    cancel: None,
                },
                terminator_span: sp(),
            },
            Block {
                instructions: vec![
                    Instruction {
                        kind: InstructionKind::StorageLive(Local(6)),
                        span: sp(),
                    },
                    Instruction {
                        kind: InstructionKind::Assign {
                            dest: Place::local(Local(6)),
                            value: Rvalue::Use(Operand::Copy(
                                Place::local(Local(4)).project(Projection::Deref),
                            )),
                        },
                        span: sp(),
                    },
                    Instruction {
                        kind: InstructionKind::StorageLive(Local(7)),
                        span: sp(),
                    },
                    Instruction {
                        kind: InstructionKind::Assign {
                            dest: Place::local(Local(7)),
                            value: Rvalue::Use(Operand::Copy(
                                Place::local(Local(5)).project(Projection::Deref),
                            )),
                        },
                        span: sp(),
                    },
                    Instruction {
                        kind: InstructionKind::StorageLive(Local(8)),
                        span: sp(),
                    },
                    Instruction {
                        kind: InstructionKind::Assign {
                            dest: Place::local(Local(8)),
                            value: Rvalue::Binary {
                                op: BinOp::Add,
                                lhs: Operand::Copy(Place::local(Local(6))),
                                rhs: Operand::Copy(Place::local(Local(7))),
                            },
                        },
                        span: sp(),
                    },
                    Instruction {
                        kind: InstructionKind::Drop {
                            place: Place::local(Local(1)),
                        },
                        span: sp(),
                    },
                    Instruction {
                        kind: InstructionKind::Drop {
                            place: Place::local(Local(0)),
                        },
                        span: sp(),
                    },
                ],
                terminator: Terminator::Return {
                    value: Operand::Move(Place::local(Local(8))),
                },
                terminator_span: sp(),
            },
        ],
    };

    Module {
        tys,
        functions: vec![Function {
            name: "disjoint_owned_stores".into(),
            sig: FnSig {
                type_params: 0,
                region_params: 0,
                params: Vec::new(),
                result: i64_ty,
                role: FnRole::Normal,
                resume_mode: ResumeMode::Local,
                state_contracts: Vec::new(),
            },
            body,
        }],
        foreigns: Vec::new(),
        statics: Vec::new(),
        dyn_impls: Vec::new(),
    }
}

/// Builds a Raw/Wire module taking two `Desc(OwnedStore<i64>)` parameters (`s0`, `s1`)
/// without any apartness proof. Both `same_param == true` (borrowing `s0` twice)
/// and `same_param == false` (borrowing `s0` then `s1` — which a caller can alias
/// by passing `(s, s)`) MUST be rejected with `ViolationKind::LoanConflict`.
fn build_two_desc_params_module(same_param: bool, exclusive_first: bool) -> Module {
    let mut tys = TyTable::new();
    let i64_ty = tys.intern(TyKind::Int(IntTy::I64));
    let store_opaque = tys.intern(TyKind::Opaque(OpaqueTy::OwnedStore { element: i64_ty }));
    let store_desc = tys.intern(TyKind::Desc(store_opaque));
    let shared_ref = tys.intern(TyKind::Ref {
        region: Region::Static,
        mutability: Mutability::Shared,
        pointee: i64_ty,
    });
    let excl_ref = tys.intern(TyKind::Ref {
        region: Region::Static,
        mutability: Mutability::Exclusive,
        pointee: i64_ty,
    });

    let (first_mut, second_mut, first_ty, second_ty) = if exclusive_first {
        (
            Mutability::Exclusive,
            Mutability::Shared,
            excl_ref,
            shared_ref,
        )
    } else {
        (
            Mutability::Shared,
            Mutability::Exclusive,
            shared_ref,
            excl_ref,
        )
    };

    let second_store = if same_param { Local(0) } else { Local(1) };

    let body = Body {
        param_count: 2,
        param_env: ParamEnv::default(),
        closure: None,
        captures: Vec::new(),
        result: i64_ty,
        locals: vec![
            LocalDecl {
                ty: store_desc,
                name: Some("s0".into()),
            },
            LocalDecl {
                ty: store_desc,
                name: Some("s1".into()),
            },
            LocalDecl {
                ty: first_ty,
                name: Some("r0".into()),
            },
            LocalDecl {
                ty: second_ty,
                name: Some("r1".into()),
            },
            LocalDecl {
                ty: i64_ty,
                name: Some("v0".into()),
            },
            LocalDecl {
                ty: i64_ty,
                name: Some("v1".into()),
            },
            LocalDecl {
                ty: i64_ty,
                name: Some("sum".into()),
            },
        ],
        drop_flags: Vec::new(),
        blocks: vec![
            Block {
                instructions: vec![Instruction {
                    kind: InstructionKind::StorageLive(Local(2)),
                    span: sp(),
                }],
                terminator: Terminator::Call {
                    callee: Callee::Intrinsic(IntrinsicOp::StoreBorrow {
                        mutability: first_mut,
                    }),
                    args: vec![
                        Operand::Copy(Place::local(Local(0))),
                        Operand::Const(ConstValue::Int {
                            ty: IntTy::I64,
                            bits: 0,
                        }),
                    ],
                    dest: Place::local(Local(2)),
                    target: BlockId(1),
                    cancel: None,
                },
                terminator_span: sp(),
            },
            Block {
                instructions: vec![Instruction {
                    kind: InstructionKind::StorageLive(Local(3)),
                    span: sp(),
                }],
                terminator: Terminator::Call {
                    callee: Callee::Intrinsic(IntrinsicOp::StoreBorrow {
                        mutability: second_mut,
                    }),
                    args: vec![
                        Operand::Copy(Place::local(second_store)),
                        Operand::Const(ConstValue::Int {
                            ty: IntTy::I64,
                            bits: 0,
                        }),
                    ],
                    dest: Place::local(Local(3)),
                    target: BlockId(2),
                    cancel: None,
                },
                terminator_span: sp(),
            },
            Block {
                instructions: vec![
                    Instruction {
                        kind: InstructionKind::StorageLive(Local(4)),
                        span: sp(),
                    },
                    Instruction {
                        kind: InstructionKind::Assign {
                            dest: Place::local(Local(4)),
                            value: Rvalue::Use(Operand::Copy(
                                Place::local(Local(2)).project(Projection::Deref),
                            )),
                        },
                        span: sp(),
                    },
                    Instruction {
                        kind: InstructionKind::StorageLive(Local(5)),
                        span: sp(),
                    },
                    Instruction {
                        kind: InstructionKind::Assign {
                            dest: Place::local(Local(5)),
                            value: Rvalue::Use(Operand::Copy(
                                Place::local(Local(3)).project(Projection::Deref),
                            )),
                        },
                        span: sp(),
                    },
                    Instruction {
                        kind: InstructionKind::StorageLive(Local(6)),
                        span: sp(),
                    },
                    Instruction {
                        kind: InstructionKind::Assign {
                            dest: Place::local(Local(6)),
                            value: Rvalue::Binary {
                                op: BinOp::Add,
                                lhs: Operand::Copy(Place::local(Local(4))),
                                rhs: Operand::Copy(Place::local(Local(5))),
                            },
                        },
                        span: sp(),
                    },
                ],
                terminator: Terminator::Return {
                    value: Operand::Move(Place::local(Local(6))),
                },
                terminator_span: sp(),
            },
        ],
    };

    Module {
        tys,
        functions: vec![Function {
            name: "two_desc_params_store_borrow".into(),
            sig: FnSig {
                type_params: 0,
                region_params: 0,
                params: vec![store_desc, store_desc],
                result: i64_ty,
                role: FnRole::Normal,
                resume_mode: ResumeMode::Local,
                state_contracts: Vec::new(),
            },
            body,
        }],
        foreigns: Vec::new(),
        statics: Vec::new(),
        dyn_impls: Vec::new(),
    }
}

/// Builds a Raw/Wire module with a branch-joined multi-definition `Desc` local `d`:
///   `if cond { d = Describe(s0) } else { d = Describe(s1) }`
/// When `overlap_borrow == false` (control), `r0` is read and dies BEFORE `r1` is
/// borrowed from `d`, so Solver produces a valid join-block certificate.
/// When `overlap_borrow == true`, `r0` (from `s0`) remains live across the second
/// `StoreBorrow` on ambiguous `d`, which MUST fail-closed with `LoanConflict`.
fn build_branch_joined_desc_module(overlap_borrow: bool, exclusive_first: bool) -> Module {
    let mut tys = TyTable::new();
    let bool_ty = tys.intern(TyKind::Bool);
    let i64_ty = tys.intern(TyKind::Int(IntTy::I64));
    let store_opaque = tys.intern(TyKind::Opaque(OpaqueTy::OwnedStore { element: i64_ty }));
    let store_managed = tys.intern(TyKind::Managed(store_opaque));
    let store_desc = tys.intern(TyKind::Desc(store_opaque));
    let shared_ref = tys.intern(TyKind::Ref {
        region: Region::Static,
        mutability: Mutability::Shared,
        pointee: i64_ty,
    });
    let excl_ref = tys.intern(TyKind::Ref {
        region: Region::Static,
        mutability: Mutability::Exclusive,
        pointee: i64_ty,
    });

    let (first_mut, second_mut, first_ty, second_ty) = if exclusive_first {
        (
            Mutability::Exclusive,
            Mutability::Shared,
            excl_ref,
            shared_ref,
        )
    } else {
        (
            Mutability::Shared,
            Mutability::Exclusive,
            shared_ref,
            excl_ref,
        )
    };

    // Locals:
    //   Local(0): cond: bool [param]
    //   Local(1): s0: Managed(OwnedStore<i64>)
    //   Local(2): s1: Managed(OwnedStore<i64>)
    //   Local(3): d_join: Desc(OwnedStore<i64>) [assigned in bb3 and bb4]
    //   Local(4): d0: Desc(OwnedStore<i64>) = Describe(s0)
    //   Local(5): r0: &i64 or &mut i64
    //   Local(6): r1: &mut i64 or &i64
    //   Local(7): v0: i64
    //   Local(8): v1: i64
    //   Local(9): sum: i64
    let bb5_insts = if overlap_borrow {
        vec![Instruction {
            kind: InstructionKind::StorageLive(Local(6)),
            span: sp(),
        }]
    } else {
        // Control: consume r0 into v0 BEFORE borrowing r1 so loans do not overlap.
        vec![
            Instruction {
                kind: InstructionKind::StorageLive(Local(7)),
                span: sp(),
            },
            Instruction {
                kind: InstructionKind::Assign {
                    dest: Place::local(Local(7)),
                    value: Rvalue::Use(Operand::Copy(
                        Place::local(Local(5)).project(Projection::Deref),
                    )),
                },
                span: sp(),
            },
            Instruction {
                kind: InstructionKind::StorageLive(Local(6)),
                span: sp(),
            },
        ]
    };

    let mut bb6_insts = Vec::new();
    if overlap_borrow {
        bb6_insts.push(Instruction {
            kind: InstructionKind::StorageLive(Local(7)),
            span: sp(),
        });
        bb6_insts.push(Instruction {
            kind: InstructionKind::Assign {
                dest: Place::local(Local(7)),
                value: Rvalue::Use(Operand::Copy(
                    Place::local(Local(5)).project(Projection::Deref),
                )),
            },
            span: sp(),
        });
    }
    bb6_insts.extend([
        Instruction {
            kind: InstructionKind::StorageLive(Local(8)),
            span: sp(),
        },
        Instruction {
            kind: InstructionKind::Assign {
                dest: Place::local(Local(8)),
                value: Rvalue::Use(Operand::Copy(
                    Place::local(Local(6)).project(Projection::Deref),
                )),
            },
            span: sp(),
        },
        Instruction {
            kind: InstructionKind::StorageLive(Local(9)),
            span: sp(),
        },
        Instruction {
            kind: InstructionKind::Assign {
                dest: Place::local(Local(9)),
                value: Rvalue::Binary {
                    op: BinOp::Add,
                    lhs: Operand::Copy(Place::local(Local(7))),
                    rhs: Operand::Copy(Place::local(Local(8))),
                },
            },
            span: sp(),
        },
        Instruction {
            kind: InstructionKind::Drop {
                place: Place::local(Local(2)),
            },
            span: sp(),
        },
        Instruction {
            kind: InstructionKind::Drop {
                place: Place::local(Local(1)),
            },
            span: sp(),
        },
    ]);

    let body = Body {
        param_count: 1,
        param_env: ParamEnv::default(),
        closure: None,
        captures: Vec::new(),
        result: i64_ty,
        locals: vec![
            LocalDecl {
                ty: bool_ty,
                name: Some("cond".into()),
            },
            LocalDecl {
                ty: store_managed,
                name: Some("s0".into()),
            },
            LocalDecl {
                ty: store_managed,
                name: Some("s1".into()),
            },
            LocalDecl {
                ty: store_desc,
                name: Some("d_join".into()),
            },
            LocalDecl {
                ty: store_desc,
                name: Some("d0".into()),
            },
            LocalDecl {
                ty: first_ty,
                name: Some("r0".into()),
            },
            LocalDecl {
                ty: second_ty,
                name: Some("r1".into()),
            },
            LocalDecl {
                ty: i64_ty,
                name: Some("v0".into()),
            },
            LocalDecl {
                ty: i64_ty,
                name: Some("v1".into()),
            },
            LocalDecl {
                ty: i64_ty,
                name: Some("sum".into()),
            },
        ],
        drop_flags: Vec::new(),
        blocks: vec![
            // bb0: s0 = StoreNew()
            Block {
                instructions: vec![Instruction {
                    kind: InstructionKind::StorageLive(Local(1)),
                    span: sp(),
                }],
                terminator: Terminator::Call {
                    callee: Callee::Intrinsic(IntrinsicOp::StoreNew),
                    args: Vec::new(),
                    dest: Place::local(Local(1)),
                    target: BlockId(1),
                    cancel: None,
                },
                terminator_span: sp(),
            },
            // bb1: s1 = StoreNew(); branch on cond
            Block {
                instructions: vec![
                    Instruction {
                        kind: InstructionKind::StorageLive(Local(2)),
                        span: sp(),
                    },
                    Instruction {
                        kind: InstructionKind::StorageLive(Local(3)),
                        span: sp(),
                    },
                ],
                terminator: Terminator::Call {
                    callee: Callee::Intrinsic(IntrinsicOp::StoreNew),
                    args: Vec::new(),
                    dest: Place::local(Local(2)),
                    target: BlockId(2),
                    cancel: None,
                },
                terminator_span: sp(),
            },
            // bb2: switch cond -> bb3 / bb4
            Block {
                instructions: Vec::new(),
                terminator: Terminator::Switch {
                    discr: Operand::Copy(Place::local(Local(0))),
                    arms: vec![(1, BlockId(3))],
                    otherwise: BlockId(4),
                },
                terminator_span: sp(),
            },
            // bb3: d_join = Describe(s0) -> bb5
            Block {
                instructions: vec![Instruction {
                    kind: InstructionKind::Assign {
                        dest: Place::local(Local(3)),
                        value: Rvalue::Describe {
                            place: Place::local(Local(1)),
                        },
                    },
                    span: sp(),
                }],
                terminator: Terminator::Goto { target: BlockId(5) },
                terminator_span: sp(),
            },
            // bb4: d_join = Describe(s1) -> bb5
            Block {
                instructions: vec![Instruction {
                    kind: InstructionKind::Assign {
                        dest: Place::local(Local(3)),
                        value: Rvalue::Describe {
                            place: Place::local(Local(2)),
                        },
                    },
                    span: sp(),
                }],
                terminator: Terminator::Goto { target: BlockId(5) },
                terminator_span: sp(),
            },
            // bb5 (join): d0 = Describe(s0); r0 = StoreBorrow(first_mut, d0) -> bb6
            Block {
                instructions: vec![
                    Instruction {
                        kind: InstructionKind::StorageLive(Local(4)),
                        span: sp(),
                    },
                    Instruction {
                        kind: InstructionKind::Assign {
                            dest: Place::local(Local(4)),
                            value: Rvalue::Describe {
                                place: Place::local(Local(1)),
                            },
                        },
                        span: sp(),
                    },
                    Instruction {
                        kind: InstructionKind::StorageLive(Local(5)),
                        span: sp(),
                    },
                ],
                terminator: Terminator::Call {
                    callee: Callee::Intrinsic(IntrinsicOp::StoreBorrow {
                        mutability: first_mut,
                    }),
                    args: vec![
                        Operand::Copy(Place::local(Local(4))),
                        Operand::Const(ConstValue::Int {
                            ty: IntTy::I64,
                            bits: 0,
                        }),
                    ],
                    dest: Place::local(Local(5)),
                    target: BlockId(6),
                    cancel: None,
                },
                terminator_span: sp(),
            },
            // bb6: r1 = StoreBorrow(second_mut, d_join) -> bb7
            Block {
                instructions: bb5_insts,
                terminator: Terminator::Call {
                    callee: Callee::Intrinsic(IntrinsicOp::StoreBorrow {
                        mutability: second_mut,
                    }),
                    args: vec![
                        Operand::Copy(Place::local(Local(3))),
                        Operand::Const(ConstValue::Int {
                            ty: IntTy::I64,
                            bits: 0,
                        }),
                    ],
                    dest: Place::local(Local(6)),
                    target: BlockId(7),
                    cancel: None,
                },
                terminator_span: sp(),
            },
            // bb7: read, sum, drop s1 & s0, return
            Block {
                instructions: bb6_insts,
                terminator: Terminator::Return {
                    value: Operand::Move(Place::local(Local(9))),
                },
                terminator_span: sp(),
            },
        ],
    };

    Module {
        tys,
        functions: vec![Function {
            name: "branch_joined_desc_store_borrow".into(),
            sig: FnSig {
                type_params: 0,
                region_params: 0,
                params: vec![bool_ty],
                result: i64_ty,
                role: FnRole::Normal,
                resume_mode: ResumeMode::Local,
                state_contracts: Vec::new(),
            },
            body,
        }],
        foreigns: Vec::new(),
        statics: Vec::new(),
        dyn_impls: Vec::new(),
    }
}

#[test]
fn raw_wire_store_borrow_conflict_rejected_and_disjoint_accepted_by_zil_check() {
    let structural_cert = Certificate {
        functions: vec![FunctionCertificate {
            function: FnId(0),
            invariants: Vec::new(),
        }],
    };

    for exclusive_first in [false, true] {
        // 1. True disjoint-owner legal control (two independent local StoreNew owners):
        //    wire roundtrip -> Solver -> independent zil-check.
        let disjoint_module = build_disjoint_owned_stores_module(exclusive_first);
        let disjoint_bytes = encode_module(&disjoint_module);
        let disjoint_decoded = decode_module(&disjoint_bytes).expect("disjoint wire decode");

        let disjoint_cert = zil_solve::solve_module(&disjoint_decoded, Budget { steps: 100_000 })
            .expect("solver must accept two genuinely distinct local StoreNew owners");
        check_module(
            disjoint_decoded.clone(),
            &disjoint_cert,
            CheckBudget { steps: 100_000 },
        )
        .expect("zil-check must accept distinct local StoreNew owners with solved cert");
        check_module(
            disjoint_decoded,
            &structural_cert,
            CheckBudget { steps: 100_000 },
        )
        .expect("zil-check must accept distinct local StoreNew owners with structural cert");

        // 2. Two Desc(Store) parameters — BOTH same_param (Local(0), Local(0)) AND
        //    distinct_params (Local(0), Local(1) without apart proof) MUST be rejected
        //    with ViolationKind::LoanConflict by BOTH Solver and independent zil-check!
        for same_param in [true, false] {
            let expected_place = if same_param {
                Place::local(Local(0))
            } else {
                Place::local(Local(1))
            };
            let conflict_module = build_two_desc_params_module(same_param, exclusive_first);
            let conflict_bytes = encode_module(&conflict_module);
            let conflict_decoded = decode_module(&conflict_bytes).expect("conflict wire decode");

            let solve_errors =
                zil_solve::solve_module(&conflict_decoded, Budget { steps: 100_000 }).expect_err(
                    "solver must reject shared+exclusive StoreBorrow across unproved Desc params",
                );
            assert!(
                solve_errors.iter().any(|err| matches!(
                    err,
                    SolveError::Violation {
                        function,
                        violation: Violation {
                            kind: ViolationKind::LoanConflict { place, .. },
                            ..
                        },
                    } if *function == FnId(0) && *place == expected_place
                )),
                "expected Solver LoanConflict on {expected_place:?} (same_param={same_param}, exclusive_first={exclusive_first}), got: {solve_errors:?}"
            );

            let check_err = check_module(
                conflict_decoded,
                &structural_cert,
                CheckBudget { steps: 100_000 },
            )
            .expect_err("zil-check must independently reject unproved Desc-param StoreBorrow");
            assert_eq!(check_err.function, FnId(0));
            assert!(
                matches!(
                    check_err.kind,
                    CheckErrorKind::Violation(Violation {
                        kind: ViolationKind::LoanConflict { ref place, .. },
                        ..
                    }) if *place == expected_place
                ),
                "expected zil-check Violation(LoanConflict on {expected_place:?}) (same_param={same_param}, exclusive_first={exclusive_first}), got: {check_err:?}"
            );
        }

        // 3. Branch-joined multi-definition Desc local (`d_join = if cond { Describe(s0) } else { Describe(s1) }`):
        //    First solve the non-overlapping control to obtain a valid join-block certificate,
        //    then verify that overlapping StoreBorrow on `s0` and `d_join` is rejected fail-closed
        //    with exact ViolationKind::LoanConflict by BOTH Solver and independent zil-check!
        let branch_ctrl = build_branch_joined_desc_module(false, exclusive_first);
        let branch_ctrl_decoded =
            decode_module(&encode_module(&branch_ctrl)).expect("branch control wire decode");
        let branch_valid_cert =
            zil_solve::solve_module(&branch_ctrl_decoded, Budget { steps: 100_000 })
                .expect("solver must accept non-overlapping branch-joined Desc control");
        check_module(
            branch_ctrl_decoded,
            &branch_valid_cert,
            CheckBudget { steps: 100_000 },
        )
        .expect("zil-check must accept non-overlapping branch-joined Desc control");

        let branch_conflict = build_branch_joined_desc_module(true, exclusive_first);
        let branch_conflict_decoded =
            decode_module(&encode_module(&branch_conflict)).expect("branch conflict wire decode");
        let branch_solve_err =
            zil_solve::solve_module(&branch_conflict_decoded, Budget { steps: 100_000 })
                .expect_err("solver must reject overlapping StoreBorrow on branch-joined Desc");
        assert!(
            branch_solve_err.iter().any(|err| matches!(
                err,
                SolveError::Violation {
                    function,
                    violation: Violation {
                        kind: ViolationKind::LoanConflict { place, .. },
                        ..
                    },
                } if *function == FnId(0) && *place == Place::local(Local(3))
            )),
            "expected Solver LoanConflict on Local(3) for branch-joined Desc (exclusive_first={exclusive_first}), got: {branch_solve_err:?}"
        );

        let branch_check_err = check_module(
            branch_conflict_decoded,
            &branch_valid_cert,
            CheckBudget { steps: 100_000 },
        )
        .expect_err("zil-check must reject overlapping branch-joined Desc with valid join cert");
        assert_eq!(branch_check_err.function, FnId(0));
        assert!(
            matches!(
                branch_check_err.kind,
                CheckErrorKind::Violation(Violation {
                    kind: ViolationKind::LoanConflict { ref place, .. },
                    ..
                }) if *place == Place::local(Local(3))
            ),
            "expected zil-check Violation(LoanConflict on Local(3)) for branch-joined Desc (exclusive_first={exclusive_first}), got: {branch_check_err:?}"
        );
    }
}

#[test]
fn source_cross_desc_store_borrow_rejected_and_distinct_owners_native() {
    let dir = temp_dir("cross-desc-source");
    let verify_opts = CompileOptions {
        stop_after: Stage::Verify,
        ..CompileOptions::default()
    };

    // 1. Negative: passing the same Store via two Desc parameters (`a: Store<i64>, b: Store<i64>`)
    //    and borrowing index 0 shared + exclusive concurrently MUST be rejected at Analyze/LoanConflict.
    let bad_src = r#"
fn bad_alias(a: Store<i64>, b: Store<i64>) -> i64 {
    let r1 = store_at(a, 0)
    let r2 = store_at_mut(b, 0)
    *r2 = 99
    return *r1
}

fn main() -> i32 {
    let s: Store<i64> = store_new()
    if !store_reserve(s, 1) { return 1 }
    store_push(s, 42)
    let _ = bad_alias(s, s)
    return 0
}
"#;
    let bad_path = dir.join("cross_desc_alias_neg.zil");
    std::fs::write(&bad_path, bad_src).expect("write bad_src");
    let bad_rep = CompilationSession::new().compile_path(&bad_path, &verify_opts);
    assert_eq!(
        bad_rep.status,
        ResultCategory::SafeReject,
        "cross-Desc StoreBorrow alias must be SafeReject, got: {:?}",
        bad_rep.diagnostics
    );
    assert!(
        bad_rep
            .diagnostics
            .iter()
            .any(|d| d.phase == Phase::Analyze && d.code == "E-SAFETY-SOLVE"),
        "expected Phase::Analyze E-SAFETY-SOLVE for cross-Desc alias, got: {:?}",
        bad_rep.diagnostics
    );

    // 2. Positive: two genuinely independent local Stores (`s0 = store_new()`, `s1 = store_new()`)
    //    borrowed concurrently with shared + exclusive `StoreBorrow` compile and run on both backends.
    let ok_src = r#"
fn main() -> i32 {
    let s0: Store<i64> = store_new()
    let s1: Store<i64> = store_new()
    if !store_reserve(s0, 1) || !store_reserve(s1, 1) { return 1 }
    store_push(s0, 10)
    store_push(s1, 20)
    let r0 = store_at(s0, 0)
    let r1 = store_at_mut(s1, 0)
    *r1 = *r1 + *r0
    if *r0 != 10 || *r1 != 30 { return 2 }
    return 0
}
"#;
    let ok_path = dir.join("distinct_owned_stores_pos.zil");
    std::fs::write(&ok_path, ok_src).expect("write ok_src");
    let (code, _) = build_and_run_dual(&dir, &ok_path);
    assert_eq!(code, 0);
    let _ = std::fs::remove_dir_all(&dir);
}
