//! End-to-end dual-backend native execution, multi-threaded contention,
//! `AtomicI32`/`AtomicI64`/`AtomicUsize`, `mutex_unlock`/`mutex_with`,
//! safety rejection, and Raw/Wire verifier tests for L3 Batch 2 (G8.5).

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use zil_base::ResultCategory;
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
    assert_ne!(
        report.status,
        ResultCategory::Success,
        "expected {} to be rejected, but it succeeded",
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
fn l3_b2_safety_negatives_rejected() {
    let root = repo_root()
        .join("tests/language-readiness/terminal/concurrency-and-registration/sync-probes");
    for name in [
        "cas-ordinary-address-reject.zil",
        "mutex-double-unlock-reject.zil",
        "mutex-view-after-unlock-reject.zil",
        "mutex-guard-view-escape-reject.zil",
        "mutex-guard-thread-reject.zil",
        "mutex-drop-tls-reject.zil",
    ] {
        check_rejected_path(&root.join(name));
    }

    // Returning `&AtomicI32` out of `atomic_scope` must be rejected by borrow escape analysis.
    let dir = temp_dir("atomic_ref_escape");
    let escape_src = r#"
fn bad()->&AtomicI32 {
    let (ok, r) = atomic_scope(0, (cell: &AtomicI32) -> &AtomicI32 => cell)
    return r
}
fn main()->i32 { return 0 }
"#;
    let escape_path = write_source(&dir, "escape.zil", escape_src);
    check_rejected_path(&escape_path);
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
