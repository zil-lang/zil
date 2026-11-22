//! Dual-backend native execution & safety negative tests for L4 Batch 3 (G10.3):
//! - C ABI (`c_callback`, POD `&Struct`/`&mut Struct` with `noescape`, all integer/float widths & NaN preservation)
//! - Floating-point runtime (`std.float` + `zil_glibc_errno_exchange`, `float_roundtrip`, `float_status`)
//! - Large closure environments (>64B mixed owning/Copy) & many-argument helpers (8 int + 8 float)
//! - Illegal C ABI safety negatives & Raw/Wire `CCallback` independent `zil-check` rejection.

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
    let dir = std::env::temp_dir().join(format!("zil-l4-abi-{name}-{}", std::process::id()));
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

fn make_wrap_clang(dir: &Path, extra_flags: &[&str]) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let script = dir.join("clang-wrap.sh");
    let flags = extra_flags.join(" ");
    std::fs::write(
        &script,
        format!("#!/usr/bin/env bash\nexec clang {flags} \"$@\"\n"),
    )
    .expect("write clang wrapper");
    let mut perms = std::fs::metadata(&script)
        .expect("stat wrapper")
        .permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&script, perms).expect("chmod wrapper");
    script
}

fn build_and_run_dual(
    dir: &Path,
    entry: &Path,
    extra_c_sources: &[PathBuf],
    extra_link_flags: &[&str],
    expected_stderr: &str,
) -> i32 {
    let clang_opt = if extra_link_flags.is_empty() {
        None
    } else {
        Some(make_wrap_clang(dir, extra_link_flags))
    };
    let mut expected: Option<i32> = None;
    for (label, backend, opt) in dual_configs() {
        let exe = dir.join(format!("bin-{label}"));
        let mut session = CompilationSession::new();
        let opts = CompileOptions {
            stop_after: Stage::Build,
            optimization: opt,
            backend,
            clang: clang_opt.clone(),
            output: Some(exe.clone()),
            extra_c_sources: extra_c_sources.to_vec(),
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
        let stderr = String::from_utf8_lossy(&out.stderr);
        if expected_stderr.is_empty() {
            assert!(
                stderr.is_empty(),
                "unexpected stderr for {} ({label}): {stderr}",
                entry.display(),
            );
        } else {
            assert!(
                stderr.contains(expected_stderr),
                "expected stderr containing `{expected_stderr}` for {} ({label}), got: {stderr}",
                entry.display(),
            );
        }
        let code = out.status.code().unwrap_or(-1);
        if let Some(prev) = expected {
            assert_eq!(code, prev, "exit mismatch on {label}");
        } else {
            expected = Some(code);
        }
    }
    expected.expect("at least one backend ran")
}

#[test]
fn l4_c_abi_pod_refs_and_c_callback_native() {
    let root = repo_root();
    let ffi_zil = root.join("tests/integers/ffi.zil");
    let ffi_c = root.join("tests/integers/ffi.c");
    let dir = temp_dir("integers_ffi");
    // `tests/integers/ffi.zil` defines `fn result() -> i32`; append `fn main() -> i32 { return result() }`.
    let mut src = std::fs::read_to_string(&ffi_zil).expect("read ffi.zil");
    src.push_str("\nfn main()->i32 { return result() }\n");
    let entry = dir.join("ffi_main.zil");
    std::fs::write(&entry, src).expect("write ffi_main.zil");
    assert_eq!(build_and_run_dual(&dir, &entry, &[ffi_c], &[], ""), 0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn l4_float_roundtrip_status_and_integer_domains_native() {
    let root = repo_root();
    let num = root.join("tests/fixtures/numeric-text");
    let ints = root.join("tests/integers");

    for (name, zil_path, c_sources, link_flags, expected_stderr) in [
        (
            "float_roundtrip",
            num.join("float_roundtrip.zil"),
            vec![num.join("float_probe.c")],
            vec![
                "-Wl,--wrap=newlocale",
                "-Wl,--wrap=uselocale",
                "-Wl,--wrap=freelocale",
            ],
            "locale created=8002 released=8002 installs=8002 restores=8002",
        ),
        (
            "float_status",
            num.join("float_status.zil"),
            vec![num.join("float_status.c")],
            vec!["-lm"],
            "",
        ),
        (
            "integer_widths",
            num.join("integer-widths.zil"),
            vec![],
            vec![],
            "",
        ),
        (
            "complete_domains",
            ints.join("complete-domains.zil"),
            vec![],
            vec![],
            "",
        ),
        (
            "store_domains",
            ints.join("store-domains.zil"),
            vec![],
            vec![],
            "",
        ),
    ] {
        let dir = temp_dir(name);
        let mut src = std::fs::read_to_string(&zil_path).expect("read fixture");
        if !src.contains("fn main(") && src.contains("fn result(") {
            src.push_str("\nfn main()->i32 { return result() }\n");
        }
        let entry = dir.join(format!("{name}.zil"));
        std::fs::write(&entry, src).expect("write fixture");
        assert_eq!(
            build_and_run_dual(&dir, &entry, &c_sources, &link_flags, expected_stderr),
            0,
            "{name} should exit 0"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[test]
fn l4_large_closure_and_many_arg_helper_abi_native() {
    let dir = temp_dir("large_closure_many_args");
    let src = r#"
fn many_args(
    a0: i64, a1: i64, a2: i64, a3: i64, a4: i64, a5: i64, a6: i64, a7: i64,
    f0: f64, f1: f64, f2: f64, f3: f64, f4: f64, f5: f64, f6: f64, f7: f64,
) -> i64 {
    let isum = a0 + a1 + a2 + a3 + a4 + a5 + a6 + a7
    let fsum = f0 + f1 + f2 + f3 + f4 + f5 + f6 + f7
    if fsum != 36.0f64 { return -1 }
    return isum
}

fn call_twice<F: FnMut(i64) -> i64>(f: &mut F, x: i64) -> i64 {
    let r1 = f(x)
    let r2 = f(x + 1)
    return r1 + r2
}

fn main() -> i32 {
    let s = many_args(
        1, 2, 3, 4, 5, 6, 7, 8,
        1.0f64, 2.0f64, 3.0f64, 4.0f64, 5.0f64, 6.0f64, 7.0f64, 8.0f64,
    )
    if s != 36 { return 1 }

    // Large capture environment (>80 bytes) mixing owned ByteBuffer + multiple scalars.
    let buf = bytes_new()
    bytes_reserve(buf, 8)
    bytes_push(buf, 7u8)
    let k0: i64 = 10
    let k1: i64 = 20
    let k2: i64 = 30
    let k3: i64 = 40
    let k4: i64 = 50
    let k5: i64 = 60
    let k6: i64 = 70
    let k7: i64 = 80
    var acc: i64 = 0
    var large_cb = move (x: i64) -> i64 => {
        acc = acc + x + k0 + k1 + k2 + k3 + k4 + k5 + k6 + k7 + bytes_len(buf)
        return acc
    }
    let total = call_twice(&mut large_cb, 1)
    // Call 1: acc = 1 + 360 + 1 = 362
    // Call 2: acc = 362 + 2 + 360 + 1 = 725
    // total = 362 + 725 = 1087
    if total != 1087 { return 2 }
    return 0
}
"#;
    let entry = dir.join("large_closure.zil");
    std::fs::write(&entry, src).expect("write large_closure.zil");
    assert_eq!(build_and_run_dual(&dir, &entry, &[], &[], ""), 0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn l4_c_abi_safety_negatives_and_raw_wire_ccallback_rejected() {
    let dir = temp_dir("abi_negatives");
    let cases: [(&str, &str, &str); 3] = [
        (
            "capturing_closure_c_callback.zil",
            r#"
extern "C" fn take_cb(cb: extern "C" fn(i32) -> i32) -> i32 contract { provides(static_callbacks()); effects(ffi); };
fn main() -> i32 {
    let k: i32 = 5
    let cl = (x: i32) -> i32 => x + k
    unsafe { return take_cb(c_callback(cl)) }
}
"#,
            "E-FFI-DECLARATION",
        ),
        (
            "non_pod_package_extern.zil",
            r#"
package Owned { manages b: ByteBuffer }
extern "C" fn bad(p: &Owned) -> i32 contract { provides(noescape(p)); effects(ffi); };
fn main() -> i32 { return 0 }
"#,
            "E-FFI-DECLARATION",
        ),
        (
            "missing_noescape_struct_ref.zil",
            r#"
struct Pair { a: i32; b: i32 }
extern "C" fn bad(p: &Pair) -> i32;
fn main() -> i32 { return 0 }
"#,
            "E-FFI-DECLARATION",
        ),
    ];
    for (fname, src, code) in cases {
        let p = dir.join(fname);
        std::fs::write(&p, src).expect("write negative");
        let mut session = CompilationSession::new();
        let report = session.compile_path(
            &p,
            &CompileOptions {
                stop_after: Stage::Verify,
                ..CompileOptions::default()
            },
        );
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{fname} must fail with SafeReject, got {:?}: {:?}",
            report.status,
            report.diagnostics
        );
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.phase == Phase::Types && d.code == code),
            "{fname} expected Types/{code}, got: {:?}",
            report.diagnostics
        );
    }

    // Independent Raw/Wire check: forged `CCallback` on a non-C-ABI FnPtr (taking `&i32`)
    // must be rejected by `zil-check`.
    use zil_base::{FileId, Span};
    use zil_ir::body::{
        Block, BlockId, Body, Callee, Certificate, ConstValue, FnId, FnRole, FnSig, Function,
        FunctionCertificate, Instruction, InstructionKind, IntTy, IntrinsicOp, Local, LocalDecl,
        Module, Mutability, OpaqueTy, Operand, ParamEnv, Place, Region, ResumeMode, Rvalue,
        Terminator, TyKind, TyTable, decode_module, encode_module,
    };

    let mut tys = TyTable::new();
    let i32_ty = tys.intern(TyKind::Int(IntTy::I32));
    let ref_i32 = tys.intern(TyKind::Ref {
        region: Region::Static,
        mutability: Mutability::Shared,
        pointee: i32_ty,
    });
    let bad_fn_ty = tys.intern(TyKind::FnPtr {
        params: vec![ref_i32],
        result: i32_ty,
    });
    let cb_ty = tys.intern(TyKind::Opaque(OpaqueTy::ForeignCallback {
        params: vec![ref_i32],
        result: i32_ty,
    }));
    let sp = Span::new(FileId(0), 0, 0);
    let body = Body {
        param_count: 1,
        locals: vec![
            LocalDecl {
                ty: bad_fn_ty,
                name: None,
            },
            LocalDecl {
                ty: cb_ty,
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
                    callee: Callee::Intrinsic(IntrinsicOp::CCallback),
                    args: vec![Operand::Copy(Place::local(Local(0)))],
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
            name: "forged_cb".into(),
            sig: FnSig {
                type_params: 0,
                region_params: 0,
                params: vec![bad_fn_ty],
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
        .expect_err("forged non-C-ABI CCallback must be rejected by zil-check");
    assert!(
        matches!(
            err.kind,
            zil_check::CheckErrorKind::Violation(zil_check::Violation {
                kind: zil_check::ViolationKind::Constraint(_),
                ..
            })
        ),
        "expected Constraint violation for forged CCallback, got: {err:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
