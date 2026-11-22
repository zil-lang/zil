//! End-to-end dual-backend native execution, multi-module generic wrapper,
//! deterministic cleanup ordering, 14 safety negative controls, and Raw/Wire
//! verifier tests for L3 Batch 3 (`c_with_callback`, G7.2 / G7.3).

use std::io::Write;
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
    let dir = std::env::temp_dir().join(format!("zil-l3-b3-{name}-{}", std::process::id()));
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

fn build_and_run_dual(
    dir: &Path,
    entry: &Path,
    extra_c: &[PathBuf],
    stdin_bytes: Option<&[u8]>,
) -> (i32, String) {
    let mut expected: Option<(i32, String)> = None;
    for (label, backend, opt) in dual_configs() {
        let exe = dir.join(format!("bin-{label}"));
        let mut session = CompilationSession::new();
        let opts = CompileOptions {
            stop_after: Stage::Build,
            optimization: opt,
            backend,
            output: Some(exe.clone()),
            extra_c_sources: extra_c.to_vec(),
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

        let mut child = Command::new(&exe)
            .stdin(if stdin_bytes.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap_or_else(|e| panic!("failed to spawn {}: {e}", exe.display()));
        if let Some(input) = stdin_bytes {
            child
                .stdin
                .as_mut()
                .expect("stdin")
                .write_all(input)
                .expect("write stdin");
        }
        let out = child.wait_with_output().expect("wait child");
        assert!(
            out.stderr.is_empty(),
            "unexpected stderr ({label}): {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let res = (
            out.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&out.stdout).into_owned(),
        );
        if let Some(prev) = &expected {
            assert_eq!(&res, prev, "backend mismatch on {label}");
        } else {
            expected = Some(res);
        }
    }
    expected.expect("at least one backend ran")
}

#[test]
fn l3_b3_direct_and_null_scoped_callback_native() {
    let root = repo_root().join("tests/foreign-scoped-callback/direct");
    let dir1 = temp_dir("direct");
    let (code1, _) = build_and_run_dual(
        &dir1,
        &root.join("direct.zil"),
        &[root.join("driver.c")],
        None,
    );
    assert_eq!(code1, 0);

    let dir2 = temp_dir("direct_null");
    let (code2, _) = build_and_run_dual(
        &dir2,
        &root.join("direct-null.zil"),
        &[root.join("driver-null.c")],
        None,
    );
    assert_eq!(code2, 0);
}

#[test]
fn l3_b3_project_disjoint_and_local_body_native() {
    let root = repo_root().join("tests/foreign-scoped-callback");
    let driver = root.join("driver.c");

    let dir1 = temp_dir("disjoint_body");
    let (code1, _) = build_and_run_dual(
        &dir1,
        &root.join("project/disjoint_body_main.zil"),
        std::slice::from_ref(&driver),
        None,
    );
    assert_eq!(code1, 0);

    let dir2 = temp_dir("local_body");
    let (code2, _) = build_and_run_dual(
        &dir2,
        &root.join("project/local_body_main.zil"),
        &[driver],
        None,
    );
    assert_eq!(code2, 0);
}

#[test]
fn l3_b3_project_main_modes_0_to_7_native() {
    let root = repo_root().join("tests/foreign-scoped-callback");
    let entry = root.join("project/main.zil");
    let driver = root.join("driver.c");

    for (label, backend, opt) in dual_configs() {
        let dir = temp_dir(&format!("proj_{label}"));
        let exe = dir.join("proj");
        let mut session = CompilationSession::new();
        let opts = CompileOptions {
            stop_after: Stage::Build,
            optimization: opt,
            backend,
            output: Some(exe.clone()),
            extra_c_sources: vec![driver.clone()],
            codegen: CodegenOptions { units: 1, jobs: 1 },
            ..CompileOptions::default()
        };
        let report = session.compile_path(&entry, &opts);
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "build failed for project/main.zil ({label}): {:?}",
            report.diagnostics
        );
        for mode in 0u8..=7u8 {
            let mut child = Command::new(&exe)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .expect("spawn proj");
            child
                .stdin
                .as_mut()
                .expect("stdin")
                .write_all(&[b'0' + mode])
                .expect("write mode");
            let out = child.wait_with_output().expect("wait proj");
            assert!(
                out.stderr.is_empty(),
                "mode {mode} ({label}) stderr: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            assert_eq!(
                out.status.code(),
                Some(0),
                "mode {mode} ({label}) failed with stdout={}",
                String::from_utf8_lossy(&out.stdout)
            );
            let stdout = String::from_utf8_lossy(&out.stdout);
            // Verify deterministic cleanup ordering on owning mode 1:
            // Body drop (DB) precedes close (C0), and callback drop (DA) follows close (Z0).
            if mode == 1 {
                let db = stdout.find("DB;").expect("body drop DB");
                let c0 = stdout.find("C0;").expect("close C0");
                let z0 = stdout.find("Z0;").expect("close end Z0");
                let da = stdout.find("DA;").expect("callback drop DA");
                assert!(
                    db < c0 && z0 < da,
                    "cleanup order violated in mode 1 ({label}): {stdout}"
                );
            }
            // Verify null-registration modes 2 and 3 drop body environment without running body.
            if mode == 2 || mode == 3 {
                assert!(
                    !stdout.contains("B1;"),
                    "null registration mode {mode} must not run body: {stdout}"
                );
                assert!(
                    stdout.contains("DB;") && stdout.contains("DA;"),
                    "null registration mode {mode} must drop both body and callback: {stdout}"
                );
            }
        }
    }
}

#[test]
fn l3_b3_all_14_negative_controls_rejected() {
    let root = repo_root().join("tests/foreign-scoped-callback/negative");
    // 13 negative controls marked `current_source_coverage: true` in RISKS.json:
    // each MUST fail as a semantic SafeReject with the exact phase, diagnostic
    // code, and reason matching RISKS.json (never Unsupported or unrelated error).
    let semantic_safe_reject_cases: [(&str, Phase, &str, &str); 13] = [
        (
            "fn_once_callback.zil",
            Phase::Types,
            "E-TYPE-MISMATCH",
            "not `FnOnce`",
        ),
        (
            "borrowed_callback_environment.zil",
            Phase::Analyze,
            "E-SAFETY-SOLVE",
            "cannot capture borrowed addresses",
        ),
        (
            "body_captures_same_callback.zil",
            Phase::Analyze,
            "E-SAFETY-SOLVE",
            "LoanConflict",
        ),
        (
            "body_moves_same_callback.zil",
            Phase::Analyze,
            "E-SAFETY-SOLVE",
            "UseOfUninit",
        ),
        (
            "missing_unsafe_region.zil",
            Phase::Types,
            "E-FFI-UNSAFE",
            "requires an `unsafe` block",
        ),
        (
            "swapped_roles.zil",
            Phase::Types,
            "E-FFI-DECLARATION",
            "scoped_callback_register()",
        ),
        (
            "missing_register_role.zil",
            Phase::Types,
            "E-FFI-DECLARATION",
            "scoped_callback_register()",
        ),
        (
            "wrong_register_signature.zil",
            Phase::Types,
            "E-FFI-DECLARATION",
            "scoped_callback_register must have signature",
        ),
        (
            "ordinary_role_call.zil",
            Phase::Types,
            "E-FFI-DECLARATION",
            "may only be passed to `c_with_callback`",
        ),
        (
            "wrong_body_result.zil",
            Phase::Types,
            "E-TYPE-MISMATCH",
            "mismatched types",
        ),
        (
            "wrong_callback_signature.zil",
            Phase::Types,
            "E-TYPE-MISMATCH",
            "mismatched types",
        ),
        (
            "body_used_after_consumption.zil",
            Phase::Analyze,
            "E-SAFETY-SOLVE",
            "UseOfUninit",
        ),
        (
            "caller_keeps_live_callback_alias.zil",
            Phase::Analyze,
            "E-SAFETY-SOLVE",
            "LoanConflict",
        ),
    ];
    for (name, expected_phase, expected_code, expected_reason) in semantic_safe_reject_cases {
        let entry = root.join(name);
        let mut session = CompilationSession::new();
        let opts = CompileOptions {
            stop_after: Stage::Verify,
            ..CompileOptions::default()
        };
        let report = session.compile_path(&entry, &opts);
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "negative control {name} must fail with SafeReject (not {:?}): {:?}",
            report.status,
            report.diagnostics
        );
        assert!(
            report.diagnostics.iter().any(|d| {
                d.phase == expected_phase
                    && d.code == expected_code
                    && d.message.contains(expected_reason)
            }),
            "negative control {name} expected ({expected_phase:?}, `{expected_code}`, `{expected_reason}`), got: {:?}",
            report.diagnostics
        );
    }

    // `body_captures_callback_alias.zil` is historically recorded with
    // `current_source_coverage: false` in RISKS.json (kept separate from the
    // 13-item group above; independent Raw/Wire proof lives in
    // `l3_provenance_closure.rs`). Verify it is also rejected specifically for
    // `LoanConflict` rather than an unrelated or Unsupported failure.
    let alias_entry = root.join("body_captures_callback_alias.zil");
    let mut session = CompilationSession::new();
    let opts = CompileOptions {
        stop_after: Stage::Verify,
        ..CompileOptions::default()
    };
    let report = session.compile_path(&alias_entry, &opts);
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "negative control body_captures_callback_alias.zil must fail with SafeReject (got {:?}): {:?}",
        report.status,
        report.diagnostics
    );
    assert!(
        report.diagnostics.iter().any(|d| {
            d.phase == Phase::Analyze
                && d.code == "E-SAFETY-SOLVE"
                && d.message.contains("LoanConflict")
        }),
        "expected LoanConflict for body_captures_callback_alias.zil, got: {:?}",
        report.diagnostics
    );
}

#[test]
fn l3_b3_raw_wire_scoped_callback_role_enforcement() {
    use zil_base::{FileId, Span};
    use zil_ir::body::{
        Block, BlockId, Body, Callee, Certificate, ConstValue, FnRole, FnSig, ForeignAbi,
        ForeignCallbackRole, ForeignDecl, ForeignId, Function, IntTy, Local, LocalDecl, Module,
        OpaqueTy, Operand, ParamEnv, Place, ResumeMode, Terminator, TyKind, TyTable, decode_module,
        encode_module,
    };

    // Forged Wire module directly calling a `ScopedClose` foreign function outside `c_with_callback`.
    let mut tys = TyTable::new();
    let unit = tys.intern(TyKind::Unit);
    let handle = tys.intern(TyKind::Opaque(OpaqueTy::ForeignHandle));
    let _i32_ty = tys.intern(TyKind::Int(IntTy::I32));
    let sp = Span::new(FileId(0), 0, 0);

    let body = Body {
        param_count: 1,
        locals: vec![
            LocalDecl {
                ty: handle,
                name: None,
            },
            LocalDecl {
                ty: unit,
                name: None,
            },
        ],
        captures: Vec::new(),
        closure: None,
        result: unit,
        param_env: ParamEnv::default(),
        drop_flags: Vec::new(),
        blocks: vec![
            Block {
                instructions: Vec::new(),
                terminator: Terminator::Call {
                    callee: Callee::Foreign(ForeignId(0)),
                    args: vec![Operand::Copy(Place::local(Local(0)))],
                    dest: Place::local(Local(1)),
                    target: BlockId(1),
                    cancel: None,
                },
                terminator_span: sp,
            },
            Block {
                instructions: Vec::new(),
                terminator: Terminator::Return {
                    value: Operand::Const(ConstValue::Unit),
                },
                terminator_span: sp,
            },
        ],
    };

    let module = Module {
        tys,
        functions: vec![Function {
            name: "forged_direct_scoped_close".into(),
            sig: FnSig {
                type_params: 0,
                region_params: 0,
                params: vec![handle],
                result: unit,
                role: FnRole::Normal,
                resume_mode: ResumeMode::Local,
                state_contracts: Vec::new(),
            },
            body,
        }],
        foreigns: vec![ForeignDecl {
            symbol: "scope_close".into(),
            abi: ForeignAbi::C,
            params: vec![handle],
            result: unit,
            noescape_params: Vec::new(),
            callback_role: Some(ForeignCallbackRole::ScopedClose),
            thread_async_driver_transfer: false,
        }],
        statics: Vec::new(),
        dyn_impls: Vec::new(),
    };

    let bytes = encode_module(&module);
    let decoded = decode_module(&bytes).expect("wire round-trip");
    assert!(
        zil_check::check_module(
            decoded,
            &Certificate::default(),
            zil_check::CheckBudget { steps: 100_000 }
        )
        .is_err(),
        "zil-check must independently reject direct Callee::Foreign call to ScopedClose provider"
    );
}
