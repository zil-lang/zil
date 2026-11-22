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

fn build_store_borrow_module(same_store: bool, exclusive_first: bool) -> Module {
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

    let second_store = if same_store { Local(0) } else { Local(1) };

    // Locals:
    //   Local(0): s0: Desc(OwnedStore<i64>)  [param]
    //   Local(1): s1: Desc(OwnedStore<i64>)  [param]
    //   Local(2): r0: &i64 or &mut i64
    //   Local(3): r1: &mut i64 or &i64
    //   Local(4): v0: i64
    //   Local(5): v1: i64
    //   Local(6): sum: i64
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
                            bits: 1,
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
            name: "store_borrow_check".into(),
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

#[test]
fn raw_wire_store_borrow_conflict_rejected_and_disjoint_accepted_by_zil_check() {
    for exclusive_first in [false, true] {
        // 1. Disjoint-store legal control: wire roundtrip -> Solver -> independent zil-check.
        let disjoint_module = build_store_borrow_module(false, exclusive_first);
        let disjoint_bytes = encode_module(&disjoint_module);
        let disjoint_decoded = decode_module(&disjoint_bytes).expect("disjoint wire decode");

        let disjoint_cert = zil_solve::solve_module(&disjoint_decoded, Budget { steps: 100_000 })
            .expect("solver must accept disjoint-store StoreBorrow control");
        check_module(
            disjoint_decoded.clone(),
            &disjoint_cert,
            CheckBudget { steps: 100_000 },
        )
        .expect("zil-check must accept disjoint-store StoreBorrow control with solved cert");

        let structural_cert = Certificate {
            functions: vec![FunctionCertificate {
                function: FnId(0),
                invariants: Vec::new(),
            }],
        };
        check_module(
            disjoint_decoded,
            &structural_cert,
            CheckBudget { steps: 100_000 },
        )
        .expect("zil-check must accept disjoint-store control with structurally complete cert");

        // 2. Same-store shared+exclusive conflict: wire roundtrip -> Solver + independent zil-check.
        let conflict_module = build_store_borrow_module(true, exclusive_first);
        let conflict_bytes = encode_module(&conflict_module);
        let conflict_decoded = decode_module(&conflict_bytes).expect("conflict wire decode");

        let solve_errors = zil_solve::solve_module(&conflict_decoded, Budget { steps: 100_000 })
            .expect_err("solver must reject same-store shared+exclusive StoreBorrow conflict");
        assert!(
            solve_errors.iter().any(|err| matches!(
                err,
                SolveError::Violation {
                    function,
                    violation: Violation {
                        kind: ViolationKind::LoanConflict { place, .. },
                        ..
                    },
                } if *function == FnId(0) && *place == Place::local(Local(0))
            )),
            "expected Solver LoanConflict on Local(0) (exclusive_first={exclusive_first}), got: {solve_errors:?}"
        );

        for (label, cert) in [
            ("structural_cert", &structural_cert),
            ("disjoint_solved_cert", &disjoint_cert),
        ] {
            let check_err = check_module(
                conflict_decoded.clone(),
                cert,
                CheckBudget { steps: 100_000 },
            )
            .expect_err("zil-check must independently reject same-store StoreBorrow conflict");
            assert_eq!(check_err.function, FnId(0));
            assert!(
                matches!(
                    check_err.kind,
                    CheckErrorKind::Violation(Violation {
                        kind: ViolationKind::LoanConflict { ref place, .. },
                        ..
                    }) if *place == Place::local(Local(0))
                ),
                "expected zil-check Violation(LoanConflict on Local(0)) with {label} (exclusive_first={exclusive_first}), got: {check_err:?}"
            );
        }
    }
}
