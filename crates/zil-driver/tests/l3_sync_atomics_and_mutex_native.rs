//! End-to-end dual-backend native execution, multi-threaded contention,
//! `AtomicI32`/`AtomicI64`/`AtomicUsize`, `mutex_unlock`/`mutex_with`,
//! safety rejection, and Raw/Wire verifier tests for L3 Batch 2 (G8.5).

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use zil_base::{Phase, ResultCategory};
use zil_driver::{
    CodegenOptions, CompilationSession, CompileOptions, NativeBackend, Optimization, Stage,
};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .to_path_buf()
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("zil-l3-b2-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn write_source(dir: &Path, filename: &str, src: &str) -> PathBuf {
    let path = dir.join(filename);
    std::fs::write(&path, src).expect("write source");
    path
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

fn build_and_run_dual(dir: &Path, entry: &Path) -> i32 {
    let mut expected: Option<i32> = None;
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
            "unexpected stderr ({label}): {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let code = out.status.code().unwrap_or(-1);
        if let Some(prev) = expected {
            assert_eq!(code, prev, "exit mismatch on {label}");
        } else {
            expected = Some(code);
        }
    }
    expected.expect("at least one backend ran")
}

fn check_rejected_path(entry: &Path) {
    let mut session = CompilationSession::new();
    let opts = CompileOptions {
        stop_after: Stage::Verify,
        ..CompileOptions::default()
    };
    let report = session.compile_path(entry, &opts);
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "expected {} to fail with SafeReject (got {:?}): {:?}",
        entry.display(),
        report.status,
        report.diagnostics
    );
    assert!(
        !report.diagnostics.is_empty(),
        "expected {} to emit at least one SafeReject diagnostic",
        entry.display()
    );
}

#[test]
fn l3_b2_atomic_scope_and_cas_contention_native() {
    let root = repo_root().join("tests/language-readiness/terminal/concurrency-and-registration");
    let cases = [
        ("atomic_scope", root.join("atomic-scope.zil")),
        ("atomic_contention", root.join("atomic-contention.zil")),
        ("cas_contention", root.join("cas-contention.zil")),
        ("cas_eval_order", root.join("cas-evaluation-order.zil")),
        ("cas_negative", root.join("cas-negative-values.zil")),
        (
            "probe_cas_observed",
            root.join("sync-probes/cas-observed-value.zil"),
        ),
        (
            "probe_lib_wrapper",
            root.join("sync-probes/library-scope-wrapper.zil"),
        ),
    ];
    for (name, path) in cases {
        let dir = temp_dir(name);
        let code = build_and_run_dual(&dir, &path);
        assert_eq!(code, 0, "{name} should exit 0");
    }
}

#[test]
fn l3_b2_atomic_i64_and_usize_native() {
    let dir = temp_dir("atomic_widths");
    let src = r#"
fn main()->i32 {
    let init64: i64 = 10000000000
    let (ok1, r1) = atomic_scope(init64, (cell: &AtomicI64) -> i32 => {
        let d: i64 = 5
        let exp: i64 = 10000000005
        let des: i64 = 10000000099
        let old = atomic_fetch_add(cell, d)
        let (obs, won) = atomic_compare_exchange(cell, exp, des)
        if old != 10000000000 || obs != 10000000005 || !won { return 2 }
        if atomic_load(cell) != 10000000099 { return 3 }
        return 0
    })
    if !ok1 || r1 != 0 { return 10 + r1 }
    let init_u: usize = 42
    let (ok2, r2) = atomic_scope(init_u, (cell: &AtomicUsize) -> i32 => {
        let s: usize = 100
        let d: usize = 23
        let bad: usize = 999
        let one: usize = 1
        atomic_store(cell, s)
        let old = atomic_fetch_add(cell, d)
        let (obs, won) = atomic_compare_exchange(cell, bad, one)
        if old != 100 || obs != 123 || won { return 4 }
        return 0
    })
    if !ok2 || r2 != 0 { return 20 + r2 }
    return 0
}
"#;
    let entry = write_source(&dir, "widths.zil", src);
    let code = build_and_run_dual(&dir, &entry);
    assert_eq!(code, 0);
}

#[test]
fn l3_b2_mutex_unlock_and_mutex_with_native() {
    let root = repo_root()
        .join("tests/language-readiness/terminal/concurrency-and-registration/sync-probes");
    let dir1 = temp_dir("mutex_owned_return");
    assert_eq!(
        build_and_run_dual(&dir1, &root.join("mutex-owned-return.zil")),
        0
    );

    let dir2 = temp_dir("mutex_protected_owner_thread");
    assert_eq!(
        build_and_run_dual(&dir2, &root.join("mutex-protected-owner-thread.zil")),
        0
    );
}

#[test]
fn l3_a_ordered_atomics_and_acquire_release_contention_native() {
    let dir = temp_dir("ordered_atomics");
    let src = r#"
fn producer(cell: &AtomicI32) -> i32 {
    var i = 0
    while i < 200 {
        atomic_fetch_add_ordered(cell, 1, MemoryOrder.AcqRel)
        i = i + 1
    }
    return 0
}
fn main() -> i32 {
    let (ok, r) = atomic_scope(0, (cell: &AtomicI32) -> i32 => {
        atomic_store_ordered(cell, 10, MemoryOrder.Release)
        let v0 = atomic_load_ordered(cell, MemoryOrder.Acquire)
        if v0 != 10 { return 1 }
        let prev = atomic_exchange_ordered(cell, 20, MemoryOrder.AcqRel)
        if prev != 10 { return 2 }
        let (obs1, won1) = atomic_compare_exchange_ordered(
            cell,
            20,
            30,
            MemoryOrder.AcqRel,
            MemoryOrder.Acquire,
        )
        if obs1 != 20 || !won1 { return 3 }
        let (obs2, won2) = atomic_compare_exchange_ordered(
            cell,
            999,
            40,
            MemoryOrder.Release,
            MemoryOrder.Relaxed,
        )
        if obs2 != 30 || won2 { return 4 }
        atomic_store_ordered(cell, 0, MemoryOrder.SeqCst)
        let (created, c_ret, p_ret) = thread_scope(producer, cell, () -> i32 => {
            var j = 0
            while j < 200 {
                atomic_fetch_add_ordered(cell, 1, MemoryOrder.AcqRel)
                j = j + 1
            }
            return 0
        })
        if !created || c_ret != 0 || p_ret != 0 { return 5 }
        let final_v = atomic_load_ordered(cell, MemoryOrder.SeqCst)
        if final_v != 400 { return 6 }
        return 0
    })
    if !ok { return 99 }
    return r
}
"#;
    let entry = write_source(&dir, "ordered_atomics.zil", src);
    assert_eq!(build_and_run_dual(&dir, &entry), 0);
}

#[test]
fn l3_b2_safety_negatives_rejected() {
    let root = repo_root()
        .join("tests/language-readiness/terminal/concurrency-and-registration/sync-probes");
    for name in [
        "cas-ordinary-address-reject.zil",
        "mutex-double-unlock-reject.zil",
        "mutex-view-after-unlock-reject.zil",
        "mutex-guard-view-escape-reject.zil",
        "mutex-guard-thread-reject.zil",
    ] {
        check_rejected_path(&root.join(name));
    }

    // Exact phase/code/reason bindings for L3-A ordered atomics, Transfer/Share,
    // raw c_handle transfer, and Mutex<Resource> foreign-Drop rejection.
    let exact_cases: [(&str, Phase, &str, &str); 4] = [
        (
            "cas-failure-order-reject.zil",
            Phase::Types,
            "E-OPERATOR-TYPE",
            "failure ordering",
        ),
        (
            "capability-copy-handle-thread-reject.zil",
            Phase::Verify,
            "E-SAFETY-CHECK",
            "raw c_handle (ForeignHandle) is not Transfer",
        ),
        (
            "capability-empty-impl-reject.zil",
            Phase::Types,
            "E-GENERIC-ARGUMENT",
            "compiler-derived capability trait",
        ),
        (
            "mutex-drop-tls-reject.zil",
            Phase::Verify,
            "E-SAFETY-CHECK",
            "Mutex payload with non-thread-safe foreign Drop",
        ),
    ];
    for (name, expected_phase, expected_code, expected_reason) in exact_cases {
        let entry = root.join(name);
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
            "{name} must fail with SafeReject, got {:?}: {:?}",
            report.status,
            report.diagnostics
        );
        assert!(
            report.diagnostics.iter().any(|d| {
                d.phase == expected_phase
                    && d.code == expected_code
                    && d.message.contains(expected_reason)
            }),
            "{name} expected ({expected_phase:?}, `{expected_code}`, `{expected_reason}`), got: {:?}",
            report.diagnostics
        );
    }

    // Invalid load/store memory orderings must also be rejected at Phase::Types.
    let dir = temp_dir("invalid_order_neg");
    for (fname, src, needle) in [
        (
            "load_release.zil",
            r#"
fn main()->i32 {
    let (ok, r) = atomic_scope(0, (cell: &AtomicI32) -> i32 => atomic_load_ordered(cell, MemoryOrder.Release))
    if !ok { return 1 } return r
}
"#,
            "atomic_load_ordered",
        ),
        (
            "store_acquire.zil",
            r#"
fn main()->i32 {
    let (ok, _) = atomic_scope(0, (cell: &AtomicI32) -> i32 => { atomic_store_ordered(cell, 1, MemoryOrder.Acquire); return 0 })
    if !ok { return 1 } return 0
}
"#,
            "atomic_store_ordered",
        ),
    ] {
        let p = write_source(&dir, fname, src);
        let mut session = CompilationSession::new();
        let report = session.compile_path(
            &p,
            &CompileOptions {
                stop_after: Stage::Verify,
                ..CompileOptions::default()
            },
        );
        assert_eq!(report.status, ResultCategory::SafeReject);
        assert!(
            report.diagnostics.iter().any(|d| d.phase == Phase::Types
                && d.code == "E-OPERATOR-TYPE"
                && d.message.contains(needle)),
            "{fname} got: {:?}",
            report.diagnostics
        );
    }

    // Returning `&AtomicI32` out of `atomic_scope` must be rejected by borrow escape analysis.
    let dir_esc = temp_dir("atomic_ref_escape");
    let escape_src = r#"
fn bad()->&AtomicI32 {
    let (ok, r) = atomic_scope(0, (cell: &AtomicI32) -> &AtomicI32 => cell)
    return r
}
fn main()->i32 { return 0 }
"#;
    let escape_path = write_source(&dir_esc, "escape.zil", escape_src);
    check_rejected_path(&escape_path);
}

#[test]
fn l3_a_raw_wire_invalid_memory_order_rejected_by_checker() {
    use zil_base::{FileId, Span};
    use zil_ir::body::{
        Block, BlockId, Body, Callee, Certificate, ConstValue, FnId, FnRole, FnSig, Function,
        FunctionCertificate, Instruction, InstructionKind, IntTy, IntrinsicOp, Local, LocalDecl,
        MemoryOrder, Module, Mutability, OpaqueTy, Operand, ParamEnv, Place, Region, ResumeMode,
        Rvalue, SyncCellKind, Terminator, TyKind, TyTable, decode_module, encode_module,
    };

    // Build a structurally valid 1-function module calling forged
    // `SyncCompareExchangeOrdered { success: Release, failure: Acquire }`.
    let mut tys = TyTable::new();
    let i32_ty = tys.intern(TyKind::Int(IntTy::I32));
    let bool_ty = tys.intern(TyKind::Bool);
    let out_ty = tys.intern(TyKind::Tuple(vec![i32_ty, bool_ty]));
    let cell_ty = tys.intern(TyKind::Opaque(OpaqueTy::SyncCell(SyncCellKind::I32)));
    let cell_ref_ty = tys.intern(TyKind::Ref {
        region: Region::Static,
        mutability: Mutability::Shared,
        pointee: cell_ty,
    });
    let sp = Span::new(FileId(0), 0, 0);
    let body = Body {
        param_count: 1,
        locals: vec![
            LocalDecl {
                ty: cell_ref_ty,
                name: None,
            },
            LocalDecl {
                ty: out_ty,
                name: None,
            },
        ],
        captures: Vec::new(),
        closure: None,
        result: i32_ty,
        param_env: ParamEnv::default(),
        drop_flags: Vec::new(),
        blocks: vec![
            Block {
                instructions: vec![Instruction {
                    kind: InstructionKind::StorageLive(Local(1)),
                    span: sp,
                }],
                terminator: Terminator::Call {
                    callee: Callee::Intrinsic(IntrinsicOp::SyncCompareExchangeOrdered {
                        success: MemoryOrder::Release,
                        failure: MemoryOrder::Acquire,
                    }),
                    args: vec![
                        Operand::Copy(Place::local(Local(0))),
                        Operand::Const(ConstValue::Int {
                            ty: IntTy::I32,
                            bits: 0,
                        }),
                        Operand::Const(ConstValue::Int {
                            ty: IntTy::I32,
                            bits: 1,
                        }),
                    ],
                    dest: Place::local(Local(1)),
                    target: BlockId(1),
                    cancel: None,
                },
                terminator_span: sp,
            },
            Block {
                instructions: vec![Instruction {
                    kind: InstructionKind::Assign {
                        dest: Place::local(Local(1)),
                        value: Rvalue::Use(Operand::Copy(Place::local(Local(1)))),
                    },
                    span: sp,
                }],
                terminator: Terminator::Return {
                    value: Operand::Const(ConstValue::Int {
                        ty: IntTy::I32,
                        bits: 0,
                    }),
                },
                terminator_span: sp,
            },
        ],
    };
    let module = Module {
        tys,
        functions: vec![Function {
            name: "forged_cas".into(),
            sig: FnSig {
                type_params: 0,
                region_params: 0,
                params: vec![cell_ref_ty],
                result: i32_ty,
                role: FnRole::Normal,
                resume_mode: ResumeMode::Local,
                state_contracts: Vec::new(),
            },
            body,
        }],
        foreigns: Vec::new(),
        statics: Vec::new(),
        dyn_impls: Vec::new(),
    };
    let bytes = encode_module(&module);
    let decoded = decode_module(&bytes).expect("wire roundtrip");
    let cert = Certificate {
        functions: vec![FunctionCertificate {
            function: FnId(0),
            invariants: Vec::new(),
        }],
    };
    let err = zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 200_000 })
        .expect_err("forged invalid CAS MemoryOrder must be rejected by zil-check");
    assert!(
        matches!(
            err.kind,
            zil_check::CheckErrorKind::Violation(zil_check::Violation {
                kind: zil_check::ViolationKind::Constraint(_),
                ..
            })
        ),
        "expected Constraint violation for forged CAS MemoryOrder, got: {err:?}"
    );
}

#[test]
fn l3_b2_raw_wire_sync_and_mutex_unlock_verification() {
    use zil_base::{FileId, Span};
    use zil_ir::body::{
        Block, BlockId, Body, Callee, Certificate, ConstValue, FnRole, FnSig, Function,
        Instruction, InstructionKind, IntTy, IntrinsicOp, Local, LocalDecl, Module, Mutability,
        OpaqueTy, Operand, ParamEnv, Place, PlaceRoot, Region, ResumeMode, Rvalue, SyncCellKind,
        Terminator, TyKind, TyTable, decode_module, encode_module,
    };

    // Wire round-trip of forged SyncCompareExchange with wrong destination type (i32 instead of (i32, bool)).
    let mut tys = TyTable::new();
    let unit = tys.intern(TyKind::Unit);
    let i32_ty = tys.intern(TyKind::Int(IntTy::I32));
    let cell_ty = tys.intern(TyKind::Opaque(OpaqueTy::SyncCell(SyncCellKind::I32)));
    let cell_ref_ty = tys.intern(TyKind::Ref {
        region: Region::Static,
        mutability: Mutability::Shared,
        pointee: cell_ty,
    });
    let sp = Span::new(FileId(0), 0, 0);
    let mut body = Body {
        param_count: 0,
        locals: vec![
            LocalDecl {
                ty: unit,
                name: None,
            },
            LocalDecl {
                ty: cell_ty,
                name: None,
            },
            LocalDecl {
                ty: cell_ref_ty,
                name: None,
            },
            LocalDecl {
                ty: i32_ty,
                name: None,
            },
        ],
        captures: Vec::new(),
        closure: None,
        result: unit,
        param_env: ParamEnv::default(),
        drop_flags: Vec::new(),
        blocks: Vec::new(),
    };

    body.blocks.push(Block {
        instructions: vec![Instruction {
            kind: InstructionKind::StorageLive(Local(1)),
            span: sp,
        }],
        terminator: Terminator::Call {
            callee: Callee::Intrinsic(IntrinsicOp::SyncInit),
            args: vec![Operand::Const(ConstValue::Int {
                ty: IntTy::I32,
                bits: 0,
            })],
            dest: Place::local(Local(1)),
            target: BlockId(1),
            cancel: None,
        },
        terminator_span: sp,
    });
    body.blocks.push(Block {
        instructions: vec![
            Instruction {
                kind: InstructionKind::StorageLive(Local(2)),
                span: sp,
            },
            Instruction {
                kind: InstructionKind::Assign {
                    dest: Place::local(Local(2)),
                    value: Rvalue::Ref {
                        region: Region::Static,
                        mutability: Mutability::Shared,
                        place: Place {
                            root: PlaceRoot::Local(Local(1)),
                            projections: Vec::new(),
                        },
                    },
                },
                span: sp,
            },
            Instruction {
                kind: InstructionKind::StorageLive(Local(3)),
                span: sp,
            },
        ],
        terminator: Terminator::Call {
            callee: Callee::Intrinsic(IntrinsicOp::SyncCompareExchange),
            args: vec![
                Operand::Copy(Place::local(Local(2))),
                Operand::Const(ConstValue::Int {
                    ty: IntTy::I32,
                    bits: 0,
                }),
                Operand::Const(ConstValue::Int {
                    ty: IntTy::I32,
                    bits: 1,
                }),
            ],
            dest: Place::local(Local(3)),
            target: BlockId(2),
            cancel: None,
        },
        terminator_span: sp,
    });
    body.blocks.push(Block {
        instructions: Vec::new(),
        terminator: Terminator::Return {
            value: Operand::Copy(Place::local(Local(0))),
        },
        terminator_span: sp,
    });

    let module = Module {
        tys,
        functions: vec![Function {
            name: "forged_cas".into(),
            sig: FnSig {
                type_params: 0,
                region_params: 0,
                params: Vec::new(),
                result: unit,
                role: FnRole::Normal,
                resume_mode: ResumeMode::Local,
                state_contracts: Vec::new(),
            },
            body,
        }],
        foreigns: Vec::new(),
        statics: Vec::new(),
        dyn_impls: Vec::new(),
    };

    let bytes = encode_module(&module);
    let decoded = decode_module(&bytes).expect("wire round-trip should succeed");
    assert!(
        zil_check::check_module(
            decoded,
            &Certificate::default(),
            zil_check::CheckBudget { steps: 100_000 }
        )
        .is_err(),
        "zil-check must independently reject forged SyncCompareExchange with non-tuple destination"
    );
}
