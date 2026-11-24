//! End-to-end native execution, source contract rejection, and Raw/Wire IR
//! verification tests for L2 Batch 1 (`task/l2-contract-semantics`, G6.1 / G5.2).

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use zil_base::ResultCategory;
use zil_driver::{
    CodegenOptions, CompilationSession, CompileOptions, NativeBackend, Optimization, Stage,
};

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("zil-l2-b1-{name}-{}", std::process::id()));
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

fn build_and_run_dual_with_c(
    dir: &Path,
    entry: &Path,
    extra_c_sources: &[PathBuf],
) -> (i32, Vec<u8>) {
    let mut expected: Option<(i32, Vec<u8>)> = None;
    for (label, backend, opt) in dual_configs() {
        let exe = dir.join(format!("bin-{label}"));
        let mut session = CompilationSession::new();
        let opts = CompileOptions {
            stop_after: Stage::Build,
            optimization: opt,
            backend,
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
        assert!(
            out.stderr.is_empty(),
            "unexpected stderr ({label}): {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let result = (out.status.code().unwrap_or(-1), out.stdout);
        if let Some(prev) = &expected {
            assert_eq!(&result, prev, "result mismatch on {label}");
        } else {
            expected = Some(result);
        }
    }
    expected.expect("at least one backend ran")
}

fn check_rejected(
    dir: &Path,
    name: &str,
    src: &str,
    expected_status: ResultCategory,
    expected_code_prefix: &str,
) {
    let entry = write_source(dir, &format!("{name}.zil"), src);
    let mut session = CompilationSession::new();
    let opts = CompileOptions {
        stop_after: Stage::Verify,
        ..CompileOptions::default()
    };
    let report = session.compile_path(&entry, &opts);
    assert_eq!(
        report.status, expected_status,
        "unexpected status for `{name}`: {:?}",
        report.diagnostics
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code.starts_with(expected_code_prefix)),
        "expected diagnostic starting with `{expected_code_prefix}` for `{name}`, got: {:?}",
        report.diagnostics
    );
}

fn check_rejected_with_reason(
    dir: &Path,
    name: &str,
    src: &str,
    expected_code_prefix: &str,
    expected_reason_substr: &str,
) {
    let entry = write_source(dir, &format!("{name}.zil"), src);
    let mut session = CompilationSession::new();
    let opts = CompileOptions {
        stop_after: Stage::Verify,
        ..CompileOptions::default()
    };
    let report = session.compile_path(&entry, &opts);
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "unexpected status for `{name}`: {:?}",
        report.diagnostics
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code.starts_with(expected_code_prefix)
                && d.message.contains(expected_reason_substr)),
        "expected `{expected_code_prefix}` containing `{expected_reason_substr}` for `{name}`, got: {:?}",
        report.diagnostics
    );
}

#[test]
fn task_resume_local_and_transfer_contracts_execute_natively_on_dual_backends() {
    let dir = temp_dir("task-contracts-native");
    let src = r#"
extern "C" fn putchar(code: i32) -> i32;

resource Token { code: i32 }
impl Drop for Token {
    fn drop(self: Self) {
        unsafe { putchar(self.code) }
    }
}

fn local_worker(give tok: Token) -> Token contract { result(give); effects(suspend); resume(local) } {
    task_yield()
    return give tok
}

fn transfer_worker(seed: i32) -> i32 contract { effects(suspend); resume(transfer) } {
    let saved = seed + 1
    task_yield()
    return saved + 4
}

fn main() -> TaskCreateError!i32 {
    var t1 = task_create(transfer_worker, 37)?
    var ans = 0
    loop {
        match task_poll(&mut t1) {
            .ok(p) => {
                match p {
                    Poll.Pending() => { continue },
                    Poll.Ready(v) => { ans = v; break }
                }
            },
            .err(_) => { break }
        }
    }
    task_request_cancel(&mut t1)
    loop {
        match task_poll_close(&mut t1) {
            Poll.Pending() => { continue },
            Poll.Ready(_) => { break }
        }
    }
    task_finish_destroy(give t1)

    let input = Token { code: 65 }
    var t2 = task_create(local_worker, give input)?
    loop {
        match task_poll(&mut t2) {
            .ok(p) => {
                match p {
                    Poll.Pending() => { continue },
                    Poll.Ready(tok) => {
                        let _ = tok
                        break
                    }
                }
            },
            .err(_) => { break }
        }
    }
    task_request_cancel(&mut t2)
    loop {
        match task_poll_close(&mut t2) {
            Poll.Pending() => { continue },
            Poll.Ready(_) => { break }
        }
    }
    task_finish_destroy(give t2)

    if ans == 42 {
        return 0
    }
    return 1
}
"#;
    let entry = write_source(&dir, "main.zil", src);
    let (code, stdout) = build_and_run_dual_with_c(&dir, &entry, &[]);
    assert_eq!(code, 0);
    assert_eq!(stdout.as_slice(), b"A");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn extern_async_callback_provider_contracts_and_protocol_state_execute_natively() {
    let dir = temp_dir("extern-contracts-native");
    let c_provider = dir.join("provider.c");
    std::fs::write(
        &c_provider,
        r#"
#include <stdint.h>
#include <stdlib.h>

typedef int64_t (*cb_fn)(void *, int32_t);
typedef struct {
    cb_fn fn;
    void *ctx;
    int drained;
} Provider;

void *p_register(cb_fn fn, void *ctx) {
    Provider *p = (Provider *)malloc(sizeof(Provider));
    if (!p) return NULL;
    p->fn = fn;
    p->ctx = ctx;
    p->drained = 0;
    return (void *)p;
}

void p_request_close(void *tok) {
    Provider *p = (Provider *)tok;
    if (p && p->fn) {
        (void)p->fn(p->ctx, 40);
    }
}

int32_t p_poll_drain(void *tok) {
    Provider *p = (Provider *)tok;
    if (!p) return 1;
    if (!p->drained) {
        p->drained = 1;
        return 0;
    }
    free(p);
    return 1;
}
"#,
    )
    .expect("write provider.c");

    let src = r#"
extern "C" fn p_register(cb: extern "C" fn(c_handle, i32) -> i64, ctx: c_handle) -> c_handle
    contract { provides(static_callbacks(), thread_async_callback_register(), thread_async_driver_transfer()); effects(ffi); };
extern "C" fn p_request_close(tok: c_handle)
    contract { provides(static_callbacks(), thread_async_callback_request_close(), thread_async_driver_transfer()); effects(ffi); };
extern "C" fn p_poll_drain(tok: c_handle) -> i32
    contract { provides(static_callbacks(), thread_async_callback_poll_drain(), thread_async_driver_transfer()); effects(ffi); };

fn close_step<T>(owner: &mut AsyncRegistration<T>) -> Poll<void>
    contract { requires(state(owner, Closing, Closed)); provides(on_pending(state(owner, Closing)), on_ready(state(owner, Closed))); }
{
    return async_registration_poll_close(owner)
}

fn retire_owner<T>(give owner: AsyncRegistration<T>)
    contract { requires(state(owner, Open, Closing, Closed)); provides(retired(owner)); }
{
    var cur = give owner
    async_registration_request_close(&mut cur)
    loop {
        match close_step(&mut cur) {
            Poll.Pending() => { continue },
            Poll.Ready(_) => { break }
        }
    }
    async_registration_finish_destroy_checked(give cur)
}

fn worker(seed: i32) -> i32 contract { effects(suspend); resume(transfer) } {
    var acc = seed
    let cb = move (v: i32) -> i32 => {
        acc += v
        return acc
    }
    unsafe {
        var reg = c_register_async(p_register, p_request_close, p_poll_drain, cb)
        task_yield()
        retire_owner(give reg)
    }
    return seed + 40
}

fn main() -> TaskCreateError!i32 {
    var t = task_create(worker, 2)?
    var out = 0
    loop {
        match task_poll(&mut t) {
            .ok(p) => {
                match p {
                    Poll.Pending() => { continue },
                    Poll.Ready(v) => { out = v; break }
                }
            },
            .err(_) => { break }
        }
    }
    task_request_cancel(&mut t)
    loop {
        match task_poll_close(&mut t) {
            Poll.Pending() => { continue },
            Poll.Ready(_) => { break }
        }
    }
    task_finish_destroy(give t)
    if out == 42 { return 0 }
    return 1
}
"#;
    let entry = write_source(&dir, "main.zil", src);
    let (code, stdout) = build_and_run_dual_with_c(&dir, &entry, &[c_provider]);
    assert_eq!(code, 0);
    assert!(stdout.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn invalid_and_unproven_contracts_are_rejected() {
    let dir = temp_dir("contract-negatives");

    // 1. Conflicting resume(local) and resume(transfer).
    check_rejected(
        &dir,
        "resume_conflict",
        r#"
fn f() -> i32 contract { resume(local); resume(transfer) } { return 0 }
fn main() -> i32 { return f() }
"#,
        ResultCategory::SafeReject,
        "E-RESOURCE-RESULT-MODE",
    );

    // 2. resume(transfer) accepting borrowed address (&i32).
    check_rejected(
        &dir,
        "resume_transfer_borrowed_param",
        r#"
fn f(x: &i32) -> i32 contract { resume(transfer) } { return *x }
fn main() -> i32 { let v = 1; return f(&v) }
"#,
        ResultCategory::SafeReject,
        "E-SAFETY-CHECK",
    );

    // 3. resume(transfer) suspending with a borrowed local across task_yield().
    check_rejected(
        &dir,
        "resume_transfer_borrowed_local_yield",
        r#"
fn f(seed: i32) -> i32 contract { effects(suspend); resume(transfer) } {
    let x = seed
    let p = &x
    task_yield()
    return *p
}
fn main() -> i32 { return 0 }
"#,
        ResultCategory::SafeReject,
        "E-SAFETY-CHECK",
    );

    // 4. Unproven effect clauses (spawn, io) remain Unsupported.
    check_rejected(
        &dir,
        "effects_spawn_unsupported",
        r#"
fn f() -> i32 contract { effects(spawn); resume(transfer) } { return 0 }
fn main() -> i32 { return f() }
"#,
        ResultCategory::Unsupported,
        "E-UNSUPPORTED",
    );
    check_rejected(
        &dir,
        "effects_unknown_unsupported",
        r#"
fn f() -> i32 contract { effects(bogus_effect) } { return 0 }
fn main() -> i32 { return f() }
"#,
        ResultCategory::Unsupported,
        "E-UNSUPPORTED",
    );

    // 5. Unknown contract clause and unknown extern provides clause remain rejected.
    check_rejected(
        &dir,
        "unknown_clause_unsupported",
        r#"
fn f() -> i32 contract { magic(foo) } { return 0 }
fn main() -> i32 { return f() }
"#,
        ResultCategory::SafeReject,
        "E-PARSE-CONTRACT-CLAUSE",
    );
    check_rejected(
        &dir,
        "unknown_extern_role_unsupported",
        r#"
extern "C" fn reg(cb: extern "C" fn(c_handle, i32) -> i64, ctx: c_handle) -> c_handle
    contract { provides(static_callbacks(), unproven_future_role()); effects(ffi); };
fn main() -> i32 { return 0 }
"#,
        ResultCategory::Unsupported,
        "E-UNSUPPORTED",
    );

    // 6. Invalid protocol-state parameter name or state name.
    check_rejected(
        &dir,
        "invalid_state_param",
        r#"
fn f(x: i32) -> i32 contract { requires(state(missing, Closed)) } { return x }
fn main() -> i32 { return f(0) }
"#,
        ResultCategory::SafeReject,
        "E-RESOURCE-RESULT-MODE",
    );
    check_rejected(
        &dir,
        "invalid_state_name",
        r#"
fn f(x: i32) -> i32 contract { requires(state(x, Bogus)) } { return x }
fn main() -> i32 { return f(0) }
"#,
        ResultCategory::SafeReject,
        "E-RESOURCE-RESULT-MODE",
    );

    // 7. Conflicting callback provider roles on one extern.
    check_rejected(
        &dir,
        "conflicting_extern_roles",
        r#"
extern "C" fn bad(tok: c_handle) -> i32
    contract { provides(async_callback_request_close(), async_callback_poll_drain()); effects(ffi); };
fn main() -> i32 { return 0 }
"#,
        ResultCategory::SafeReject,
        "E-FFI-DECLARATION",
    );

    // 8. Swapped callback provider roles in c_register_async.
    check_rejected(
        &dir,
        "swapped_async_callback_roles",
        r#"
extern "C" fn p_reg(cb: extern "C" fn(c_handle, i32) -> i64, ctx: c_handle) -> c_handle
    contract { provides(static_callbacks(), async_callback_poll_drain()); effects(ffi); };
extern "C" fn p_req(tok: c_handle)
    contract { provides(static_callbacks(), async_callback_request_close()); effects(ffi); };
extern "C" fn p_drain(tok: c_handle) -> i32
    contract { provides(static_callbacks(), async_callback_poll_drain()); effects(ffi); };
fn main() -> i32 {
    let cb = move (v: i32) -> i32 => v + 1
    unsafe {
        var r = c_register_async(p_reg, p_req, p_drain, cb)
        async_registration_request_close(&mut r)
        loop {
            match async_registration_poll_close(&mut r) {
                Poll.Pending() => { continue },
                Poll.Ready(_) => { break }
            }
        }
        async_registration_finish_destroy_checked(give r)
    }
    return 0
}
"#,
        ResultCategory::SafeReject,
        "E-SAFETY-CHECK",
    );

    // 9. Mixing single-thread and threaded callback roles in c_register_async.
    check_rejected(
        &dir,
        "mixed_async_callback_families",
        r#"
extern "C" fn p_reg(cb: extern "C" fn(c_handle, i32) -> i64, ctx: c_handle) -> c_handle
    contract { provides(static_callbacks(), thread_async_callback_register()); effects(ffi); };
extern "C" fn p_req(tok: c_handle)
    contract { provides(static_callbacks(), async_callback_request_close()); effects(ffi); };
extern "C" fn p_drain(tok: c_handle) -> i32
    contract { provides(static_callbacks(), async_callback_poll_drain()); effects(ffi); };
fn main() -> i32 {
    let cb = move (v: i32) -> i32 => v + 1
    unsafe {
        var r = c_register_async(p_reg, p_req, p_drain, cb)
        async_registration_request_close(&mut r)
        loop {
            match async_registration_poll_close(&mut r) {
                Poll.Pending() => { continue },
                Poll.Ready(_) => { break }
            }
        }
        async_registration_finish_destroy_checked(give r)
    }
    return 0
}
"#,
        ResultCategory::SafeReject,
        "E-SAFETY-CHECK",
    );

    // 10. Non-transfer async provider inside a resume(transfer) function.
    check_rejected(
        &dir,
        "non_transfer_provider_in_resume_transfer",
        r#"
extern "C" fn p_reg(cb: extern "C" fn(c_handle, i32) -> i64, ctx: c_handle) -> c_handle
    contract { provides(static_callbacks(), async_callback_register()); effects(ffi); };
extern "C" fn p_req(tok: c_handle)
    contract { provides(static_callbacks(), async_callback_request_close()); effects(ffi); };
extern "C" fn p_drain(tok: c_handle) -> i32
    contract { provides(static_callbacks(), async_callback_poll_drain()); effects(ffi); };
fn worker() -> i32 contract { effects(suspend); resume(transfer) } {
    let cb = move (v: i32) -> i32 => v + 1
    unsafe {
        var r = c_register_async(p_reg, p_req, p_drain, cb)
        task_yield()
        async_registration_request_close(&mut r)
        loop {
            match async_registration_poll_close(&mut r) {
                Poll.Pending() => { continue },
                Poll.Ready(_) => { break }
            }
        }
        async_registration_finish_destroy_checked(give r)
    }
    return 0
}
fn main() -> i32 { return 0 }
"#,
        ResultCategory::SafeReject,
        "E-SAFETY-CHECK",
    );

    // 11. Transitive suspend via helper holding borrowed local across call in resume(transfer).
    check_rejected(
        &dir,
        "resume_transfer_indirect_helper_borrowed_local",
        r#"
fn helper() { task_yield() }
fn worker(seed: i32) -> i32 contract { effects(suspend); resume(transfer) } {
    let x = seed
    let p = &x
    helper()
    return *p
}
fn main() -> i32 { return 0 }
"#,
        ResultCategory::SafeReject,
        "E-SAFETY-CHECK",
    );

    // 12. Transitive suspend helper accepting borrowed &i32 called from resume(transfer).
    check_rejected(
        &dir,
        "resume_transfer_indirect_helper_borrowed_param",
        r#"
fn helper(p: &i32) -> i32 {
    task_yield()
    return *p
}
fn worker(seed: i32) -> i32 contract { effects(suspend); resume(transfer) } {
    let x = seed
    return helper(&x)
}
fn main() -> i32 { return 0 }
"#,
        ResultCategory::SafeReject,
        "E-SAFETY-CHECK",
    );

    // 13. Fail-closed: unannotated extern "C" fn (missing callback_role) in resume(transfer).
    check_rejected(
        &dir,
        "unannotated_extern_provider_in_resume_transfer",
        r#"
extern "C" fn p_reg(cb: extern "C" fn(c_handle, i32) -> i64, ctx: c_handle) -> c_handle;
extern "C" fn p_req(tok: c_handle);
extern "C" fn p_drain(tok: c_handle) -> i32;
fn worker() -> i32 contract { effects(suspend); resume(transfer) } {
    let cb = move (v: i32) -> i32 => v + 1
    unsafe {
        var r = c_register_async(p_reg, p_req, p_drain, cb)
        task_yield()
        async_registration_request_close(&mut r)
        loop {
            match async_registration_poll_close(&mut r) {
                Poll.Pending() => { continue },
                Poll.Ready(_) => { break }
            }
        }
        async_registration_finish_destroy_checked(give r)
    }
    return 0
}
fn main() -> i32 { return 0 }
"#,
        ResultCategory::SafeReject,
        "E-SAFETY-CHECK",
    );

    // 14. Fail-closed: thread_async_callback_* without thread_async_driver_transfer in resume(transfer).
    check_rejected(
        &dir,
        "missing_driver_transfer_in_resume_transfer",
        r#"
extern "C" fn p_reg(cb: extern "C" fn(c_handle, i32) -> i64, ctx: c_handle) -> c_handle
    contract { provides(static_callbacks(), thread_async_callback_register()); effects(ffi); };
extern "C" fn p_req(tok: c_handle)
    contract { provides(static_callbacks(), thread_async_callback_request_close()); effects(ffi); };
extern "C" fn p_drain(tok: c_handle) -> i32
    contract { provides(static_callbacks(), thread_async_callback_poll_drain()); effects(ffi); };
fn worker() -> i32 contract { effects(suspend); resume(transfer) } {
    let cb = move (v: i32) -> i32 => v + 1
    unsafe {
        var r = c_register_async(p_reg, p_req, p_drain, cb)
        task_yield()
        async_registration_request_close(&mut r)
        loop {
            match async_registration_poll_close(&mut r) {
                Poll.Pending() => { continue },
                Poll.Ready(_) => { break }
            }
        }
        async_registration_finish_destroy_checked(give r)
    }
    return 0
}
fn main() -> i32 { return 0 }
"#,
        ResultCategory::SafeReject,
        "E-SAFETY-CHECK",
    );

    // 15. Protocol state contract on non-protocol parameter type (i32).
    check_rejected(
        &dir,
        "state_contract_on_non_protocol_param",
        r#"
fn bad(x: i32) -> i32 contract { requires(state(x, Closed)) } { return x }
fn main() -> i32 { return bad(0) }
"#,
        ResultCategory::SafeReject,
        "E-RESOURCE-RESULT-MODE",
    );

    // 16. Unfulfilled provides(on_ready(state(owner, Closed))): fabricating Poll.Ready on Open handle.
    check_rejected(
        &dir,
        "fabricated_poll_ready_violates_closed_contract",
        r#"
fn bad<T>(owner: &mut AsyncRegistration<T>) -> Poll<void>
    contract { requires(state(owner, Open)); provides(on_ready(state(owner, Closed))); }
{
    return Poll.Ready(())
}
extern "C" fn p_reg(cb: extern "C" fn(c_handle, i32) -> i64, ctx: c_handle) -> c_handle;
extern "C" fn p_req(tok: c_handle);
extern "C" fn p_drain(tok: c_handle) -> i32;
fn main() -> i32 {
    let cb = move (v: i32) -> i32 => v + 1
    unsafe {
        var r = c_register_async(p_reg, p_req, p_drain, cb)
        let _ = bad(&mut r)
        async_registration_request_close(&mut r)
        loop {
            match async_registration_poll_close(&mut r) {
                Poll.Pending() => { continue },
                Poll.Ready(_) => { break }
            }
        }
        async_registration_finish_destroy_checked(give r)
    }
    return 0
}
"#,
        ResultCategory::SafeReject,
        "E-SAFETY-CHECK",
    );

    // 17. Unfulfilled provides(retired(owner)): calling finish_destroy without proving Closed.
    check_rejected(
        &dir,
        "unproved_closed_before_finish_destroy",
        r#"
fn bad<T>(give owner: AsyncRegistration<T>)
    contract { requires(state(owner, Open)); provides(retired(owner)); }
{
    var cur = give owner
    async_registration_request_close(&mut cur)
    async_registration_finish_destroy_checked(give cur)
}
extern "C" fn p_reg(cb: extern "C" fn(c_handle, i32) -> i64, ctx: c_handle) -> c_handle;
extern "C" fn p_req(tok: c_handle);
extern "C" fn p_drain(tok: c_handle) -> i32;
fn main() -> i32 {
    let cb = move (v: i32) -> i32 => v + 1
    unsafe {
        let r = c_register_async(p_reg, p_req, p_drain, cb)
        bad(give r)
    }
    return 0
}
"#,
        ResultCategory::SafeReject,
        "E-SAFETY-CHECK",
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn raw_wire_forged_resume_transfer_with_borrowed_param_is_rejected_by_checker() {
    use zil_ir::body::{
        Block, Body, ConstValue, FnRole, FnSig, Function, IntTy, LocalDecl, Module, Mutability,
        Operand, ParamEnv, Region, ResumeMode, StateContractClause, Terminator, TyKind, TyTable,
        decode_module, encode_module,
    };

    let mut tys = TyTable::new();
    let i32_ty = tys.intern(TyKind::Int(IntTy::I32));
    let ref_i32 = tys.intern(TyKind::Ref {
        region: Region::Static,
        mutability: Mutability::Shared,
        pointee: i32_ty,
    });

    let forged = Function {
        name: "forged_transfer".into(),
        sig: FnSig {
            params: vec![ref_i32],
            result: i32_ty,
            type_params: 0,
            region_params: 0,
            role: FnRole::Normal,
            resume_mode: ResumeMode::Transfer,
            state_contracts: Vec::new(),
        },
        body: Body {
            param_count: 1,
            locals: vec![LocalDecl {
                ty: ref_i32,
                name: Some("p".into()),
            }],
            captures: vec![],
            closure: None,
            result: i32_ty,
            param_env: ParamEnv::default(),
            drop_flags: vec![],
            blocks: vec![Block {
                instructions: vec![],
                terminator: Terminator::Return {
                    value: Operand::Const(ConstValue::Int {
                        ty: IntTy::I32,
                        bits: 0,
                    }),
                },
                terminator_span: zil_base::Span::default(),
            }],
        },
    };

    let module = Module {
        tys,
        functions: vec![forged],
        foreigns: vec![],
        statics: vec![],
        dyn_impls: vec![],
    };

    let bytes = encode_module(&module);
    let decoded = decode_module(&bytes).expect("wire decode forged transfer module");
    let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
        .expect("solver produces certificate for otherwise borrow-valid body");
    let checked =
        zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 1_000_000 });
    assert!(
        checked.is_err(),
        "independent checker must reject Raw/Wire ResumeMode::Transfer function accepting borrowed &i32"
    );

    // Also verify Raw/Wire forged ProvidesRetired on a non-protocol or unretired param is rejected.
    let mut tys2 = TyTable::new();
    let i32_ty2 = tys2.intern(TyKind::Int(IntTy::I32));
    let forged_state = Function {
        name: "forged_retired".into(),
        sig: FnSig {
            params: vec![i32_ty2],
            result: i32_ty2,
            type_params: 0,
            region_params: 0,
            role: FnRole::Normal,
            resume_mode: ResumeMode::Local,
            state_contracts: vec![StateContractClause::ProvidesRetired { param: 0 }],
        },
        body: Body {
            param_count: 1,
            locals: vec![LocalDecl {
                ty: i32_ty2,
                name: Some("x".into()),
            }],
            captures: vec![],
            closure: None,
            result: i32_ty2,
            param_env: ParamEnv::default(),
            drop_flags: vec![],
            blocks: vec![Block {
                instructions: vec![],
                terminator: Terminator::Return {
                    value: Operand::Const(ConstValue::Int {
                        ty: IntTy::I32,
                        bits: 0,
                    }),
                },
                terminator_span: zil_base::Span::default(),
            }],
        },
    };
    let module2 = Module {
        tys: tys2,
        functions: vec![forged_state],
        foreigns: vec![],
        statics: vec![],
        dyn_impls: vec![],
    };
    let bytes2 = encode_module(&module2);
    let decoded2 = decode_module(&bytes2).expect("wire decode forged state module");
    let cert2 = zil_solve::solve_module(&decoded2, zil_solve::Budget { steps: 1_000_000 })
        .expect("solver produces certificate");
    let checked2 = zil_check::check_module(
        decoded2,
        &cert2,
        zil_check::CheckBudget { steps: 1_000_000 },
    );
    assert!(
        checked2.is_err(),
        "independent checker must reject Raw/Wire forged StateContractClause"
    );
}

#[test]
fn user_state_protocol_contracts_execute_natively_on_dual_backends() {
    let dir = temp_dir("user-state-protocol-native");
    let src = r#"
extern "C" fn putchar(code: i32) -> i32;

enum ConnState { Init, Open, Closed }
enum ConnErr { Closed, Busy }

struct Conn {
    state: ConnState,
    fd: i32,
}

fn open(c: &mut Conn, fail: bool) -> ConnErr!void
    contract {
        requires(state(c, ConnState.Init));
        provides(on_ok(state(c, ConnState.Open)), on_err(state(c, ConnState.Init)));
    }
{
    if fail {
        return ConnErr.Busy
    }
    c.state = ConnState.Open
    return ()
}

fn send(c: &mut Conn, byte: i32)
    contract {
        requires(state(c, ConnState.Open));
        provides(state(c, ConnState.Open));
    }
{
    unsafe { putchar(byte) }
}

fn close(c: &mut Conn)
    contract {
        requires(state(c, ConnState.Open));
        provides(state(c, ConnState.Closed));
    }
{
    c.state = ConnState.Closed
}

fn ensure_open(c: &mut Conn) -> ConnErr!void
    contract {
        requires(state(c, ConnState.Init, ConnState.Open, ConnState.Closed));
        provides(on_ok(state(c, ConnState.Open)), on_err(state(c, ConnState.Init, ConnState.Closed)));
    }
{
    match c.state {
        ConnState.Init() => {
            open(c, false)?
            return ()
        },
        ConnState.Open() => {
            return ()
        },
        ConnState.Closed() => {
            return ConnErr.Closed
        }
    }
}

fn transfer_pair(src: &mut Conn, dst: &mut Conn)
    contract {
        requires(state(src, ConnState.Open), state(dst, ConnState.Init), apart(src, dst));
        provides(state(src, ConnState.Closed), state(dst, ConnState.Open));
    }
{
    src.state = ConnState.Closed
    dst.state = ConnState.Open
}

fn compare_distinct(a: &Conn, b: &Conn) -> i32
    contract {
        requires(state(a, ConnState.Open), state(b, ConnState.Closed), apart(a, b));
    }
{
    return a.fd - b.fd
}

fn main() -> i32 {
    var c1 = Conn { state: ConnState.Init, fd: 10 }
    var c2 = Conn { state: ConnState.Init, fd: 52 }

    // Failed open preserves Init state via on_err.
    match open(&mut c1, true) {
        .ok(_) => { return 1 },
        .err(_) => {}
    }

    // Branch-refined ensure_open transitions Init -> Open.
    match ensure_open(&mut c1) {
        .ok(_) => {},
        .err(_) => { return 2 }
    }
    send(&mut c1, 65)

    // Calling ensure_open again when already Open stays Open.
    match ensure_open(&mut c1) {
        .ok(_) => {},
        .err(_) => { return 3 }
    }
    send(&mut c1, 66)

    // Multi-role protocol transition with apart(src, dst).
    transfer_pair(&mut c1, &mut c2)
    let diff = compare_distinct(&c2, &c1)
    send(&mut c2, 67)
    close(&mut c2)

    if diff == 42 {
        return 0
    }
    return 4
}
"#;
    let entry = write_source(&dir, "main.zil", src);
    let (code, stdout) = build_and_run_dual_with_c(&dir, &entry, &[]);
    assert_eq!(code, 0);
    assert_eq!(stdout.as_slice(), b"ABC");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn user_state_protocol_violations_are_rejected() {
    let dir = temp_dir("user-state-protocol-reject");

    // 1. Unannotated main() violates requires(state(c, Open)) by calling send on Init.
    check_rejected(
        &dir,
        "unannotated_caller_violates_precondition",
        r#"
enum ConnState { Init, Open, Closed }
struct Conn { state: ConnState, fd: i32 }
fn send(c: &mut Conn) contract { requires(state(c, ConnState.Open)); provides(state(c, ConnState.Open)); } {}
fn main() -> i32 {
    var c = Conn { state: ConnState.Init, fd: 1 }
    send(&mut c)
    return 0
}
"#,
        ResultCategory::SafeReject,
        "E-SAFETY-CHECK",
    );

    // 2. Calling send in the .err branch of open() (where on_err leaves state as Init).
    check_rejected(
        &dir,
        "on_err_branch_cannot_assume_open",
        r#"
enum ConnState { Init, Open, Closed }
enum ConnErr { Busy }
struct Conn { state: ConnState }
fn open(c: &mut Conn) -> ConnErr!void
    contract {
        requires(state(c, ConnState.Init));
        provides(on_ok(state(c, ConnState.Open)), on_err(state(c, ConnState.Init)));
    }
{
    return ConnErr.Busy
}
fn send(c: &mut Conn) contract { requires(state(c, ConnState.Open)); } {}
fn main() -> i32 {
    var c = Conn { state: ConnState.Init }
    match open(&mut c) {
        .ok(_) => { send(&mut c) },
        .err(_) => { send(&mut c) }
    }
    return 0
}
"#,
        ResultCategory::SafeReject,
        "E-SAFETY-CHECK",
    );

    // 3. Function declares provides(state(c, Open)) but misses transition on one branch.
    check_rejected(
        &dir,
        "missing_transition_on_branch_violates_provides_state",
        r#"
enum ConnState { Init, Open, Closed }
struct Conn { state: ConnState }
fn broken_open(c: &mut Conn, flag: bool)
    contract {
        requires(state(c, ConnState.Init));
        provides(state(c, ConnState.Open));
    }
{
    if flag {
        c.state = ConnState.Open
    }
}
fn main() -> i32 {
    var c = Conn { state: ConnState.Init }
    broken_open(&mut c, false)
    return 0
}
"#,
        ResultCategory::SafeReject,
        "E-SAFETY-CHECK",
    );

    // 4. Function declares provides(on_ok(state(c, Open))) but returns ok without updating state.
    check_rejected(
        &dir,
        "forged_on_ok_without_state_transition",
        r#"
enum ConnState { Init, Open, Closed }
enum ConnErr { Busy }
struct Conn { state: ConnState }
fn lying_open(c: &mut Conn) -> ConnErr!void
    contract {
        requires(state(c, ConnState.Init));
        provides(on_ok(state(c, ConnState.Open)), on_err(state(c, ConnState.Init)));
    }
{
    return ()
}
fn main() -> i32 {
    var c = Conn { state: ConnState.Init }
    let _ = lying_open(&mut c)
    return 0
}
"#,
        ResultCategory::SafeReject,
        "E-SAFETY-CHECK",
    );

    // 5. Violating requires(apart(a, b)) with two shared references to the same handle.
    check_rejected(
        &dir,
        "aliasing_shared_refs_violates_apart",
        r#"
enum ConnState { Init, Open, Closed }
struct Conn { state: ConnState }
fn pair_check(a: &Conn, b: &Conn) -> i32
    contract { requires(state(a, ConnState.Open), state(b, ConnState.Open), apart(a, b)); }
{
    return 0
}
fn main() -> i32 {
    let c = Conn { state: ConnState.Open }
    return pair_check(&c, &c)
}
"#,
        ResultCategory::SafeReject,
        "E-SAFETY-CHECK",
    );

    // 6. Forwarding two shared parameters without declaring requires(apart(a, b)) is rejected.
    check_rejected(
        &dir,
        "unproven_shared_params_cannot_satisfy_apart",
        r#"
enum ConnState { Init, Open, Closed }
struct Conn { state: ConnState }
fn pair_check(a: &Conn, b: &Conn) -> i32
    contract { requires(state(a, ConnState.Open), state(b, ConnState.Open), apart(a, b)); }
{
    return 0
}
fn leaky_wrapper(a: &Conn, b: &Conn) -> i32
    contract { requires(state(a, ConnState.Open), state(b, ConnState.Open)); }
{
    return pair_check(a, b)
}
fn main() -> i32 {
    let c = Conn { state: ConnState.Open }
    return leaky_wrapper(&c, &c)
}
"#,
        ResultCategory::SafeReject,
        "E-SAFETY-CHECK",
    );

    // 7. Unknown unannotated mutable call conservatively widens state, blocking subsequent send().
    check_rejected(
        &dir,
        "unknown_mut_call_widens_protocol_state",
        r#"
enum ConnState { Init, Open, Closed }
struct Conn { state: ConnState }
fn mutate_unknown(c: &mut Conn) {
    c.state = ConnState.Closed
}
fn send(c: &mut Conn) contract { requires(state(c, ConnState.Open)); } {}
fn main() -> i32 {
    var c = Conn { state: ConnState.Open }
    mutate_unknown(&mut c)
    send(&mut c)
    return 0
}
"#,
        ResultCategory::SafeReject,
        "E-SAFETY-CHECK",
    );

    // 8. Unknown variant name in state(...) contract is rejected at typecheck.
    check_rejected(
        &dir,
        "unknown_user_state_variant_rejected",
        r#"
enum ConnState { Init, Open, Closed }
struct Conn { state: ConnState }
fn bad(c: &mut Conn) contract { requires(state(c, ConnState.Bogus)); } {}
fn main() -> i32 { return 0 }
"#,
        ResultCategory::SafeReject,
        "E-RESOURCE-RESULT-MODE",
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn raw_wire_forged_user_state_protocol_contracts_are_rejected_by_checker() {
    use zil_ir::body::{
        AdtDef, AdtKind, Block, BlockId, Body, Callee, ConstValue, FieldDef, FnId, FnRole, FnSig,
        Function, GenericArgs, IntTy, Local, LocalDecl, Module, Mutability, Operand, ParamEnv,
        Place, ProtocolStateSet, Region, ResumeMode, StateContractClause, Terminator, TyKind,
        TyTable, VariantDef, decode_module, encode_module,
    };

    // Helper to register `enum ConnState { Init, Open, Closed }` and `struct Conn { state: ConnState }`.
    let make_conn_types = || {
        let mut tys = TyTable::new();
        let unit_ty = tys.intern(TyKind::Unit);
        let empty_args = tys.intern_args(GenericArgs {
            tys: vec![],
            regions: vec![],
        });
        let state_adt = tys.add_adt(AdtDef {
            name: "ConnState".into(),
            kind: AdtKind::Enum,
            type_params: 0,
            region_params: 0,
            variants: vec![
                VariantDef {
                    name: "Init".into(),
                    fields: vec![],
                },
                VariantDef {
                    name: "Open".into(),
                    fields: vec![],
                },
                VariantDef {
                    name: "Closed".into(),
                    fields: vec![],
                },
            ],
            user_drop: false,
        });
        let state_ty = tys.intern(TyKind::Adt {
            adt: state_adt,
            args: empty_args,
        });
        let conn_adt = tys.add_adt(AdtDef {
            name: "Conn".into(),
            kind: AdtKind::Record,
            type_params: 0,
            region_params: 0,
            variants: vec![VariantDef {
                name: "Conn".into(),
                fields: vec![FieldDef {
                    name: "state".into(),
                    ty: state_ty,
                }],
            }],
            user_drop: false,
        });
        let conn_ty = tys.intern(TyKind::Adt {
            adt: conn_adt,
            args: empty_args,
        });
        let mut_conn = tys.intern(TyKind::Ref {
            region: Region::Static,
            mutability: Mutability::Exclusive,
            pointee: conn_ty,
        });
        let ref_conn = tys.intern(TyKind::Ref {
            region: Region::Static,
            mutability: Mutability::Shared,
            pointee: conn_ty,
        });
        (tys, unit_ty, mut_conn, ref_conn)
    };

    // Case A: Raw/Wire forged `ProvidesState(Open)` with no state transition in body.
    let (tys_a, unit_a, mut_conn_a, _) = make_conn_types();
    let forged_open = Function {
        name: "forged_open".into(),
        sig: FnSig {
            params: vec![mut_conn_a],
            result: unit_a,
            type_params: 0,
            region_params: 0,
            role: FnRole::Normal,
            resume_mode: ResumeMode::Local,
            state_contracts: vec![
                StateContractClause::RequiresState {
                    param: 0,
                    states: ProtocolStateSet::single(0),
                },
                StateContractClause::ProvidesState {
                    param: 0,
                    states: ProtocolStateSet::single(1),
                },
            ],
        },
        body: Body {
            param_count: 1,
            locals: vec![LocalDecl {
                ty: mut_conn_a,
                name: Some("c".into()),
            }],
            captures: vec![],
            closure: None,
            result: unit_a,
            param_env: ParamEnv::default(),
            drop_flags: vec![],
            blocks: vec![Block {
                instructions: vec![],
                terminator: Terminator::Return {
                    value: Operand::Const(ConstValue::Unit),
                },
                terminator_span: zil_base::Span::default(),
            }],
        },
    };
    let mod_a = Module {
        tys: tys_a,
        functions: vec![forged_open],
        foreigns: vec![],
        statics: vec![],
        dyn_impls: vec![],
    };
    let bytes_a = encode_module(&mod_a);
    let dec_a = decode_module(&bytes_a).expect("wire decode forged ProvidesState module");
    let cert_a = zil_solve::solve_module(&dec_a, zil_solve::Budget { steps: 1_000_000 })
        .expect("solver produces valid borrow certificate for forged ProvidesState");
    let err_a =
        zil_check::check_module(dec_a, &cert_a, zil_check::CheckBudget { steps: 1_000_000 })
            .expect_err("checker must reject forged ProvidesState");
    assert_eq!(err_a.function, FnId(0));
    assert!(
        matches!(err_a.kind, zil_check::CheckErrorKind::CertificateShape(_)),
        "expected CertificateShape rejection, got {:?}",
        err_a.kind
    );

    // Case B: Raw/Wire caller violating callee `RequiresApart(0, 1)` by passing same Local(0) twice.
    let (mut tys_b, unit_b, _, ref_conn_b) = make_conn_types();
    let i32_b = tys_b.intern(TyKind::Int(IntTy::I32));
    let callee_apart = Function {
        name: "callee_apart".into(),
        sig: FnSig {
            params: vec![ref_conn_b, ref_conn_b],
            result: i32_b,
            type_params: 0,
            region_params: 0,
            role: FnRole::Normal,
            resume_mode: ResumeMode::Local,
            state_contracts: vec![StateContractClause::RequiresApart {
                param_a: 0,
                param_b: 1,
            }],
        },
        body: Body {
            param_count: 2,
            locals: vec![
                LocalDecl {
                    ty: ref_conn_b,
                    name: Some("a".into()),
                },
                LocalDecl {
                    ty: ref_conn_b,
                    name: Some("b".into()),
                },
            ],
            captures: vec![],
            closure: None,
            result: i32_b,
            param_env: ParamEnv::default(),
            drop_flags: vec![],
            blocks: vec![Block {
                instructions: vec![],
                terminator: Terminator::Return {
                    value: Operand::Const(ConstValue::Int {
                        ty: IntTy::I32,
                        bits: 0,
                    }),
                },
                terminator_span: zil_base::Span::default(),
            }],
        },
    };
    let caller_aliasing = Function {
        name: "caller_aliasing".into(),
        sig: FnSig {
            params: vec![ref_conn_b],
            result: unit_b,
            type_params: 0,
            region_params: 0,
            role: FnRole::Normal,
            resume_mode: ResumeMode::Local,
            state_contracts: vec![],
        },
        body: Body {
            param_count: 1,
            locals: vec![
                LocalDecl {
                    ty: ref_conn_b,
                    name: Some("c".into()),
                },
                LocalDecl {
                    ty: i32_b,
                    name: Some("tmp".into()),
                },
            ],
            captures: vec![],
            closure: None,
            result: unit_b,
            param_env: ParamEnv::default(),
            drop_flags: vec![],
            blocks: vec![
                Block {
                    instructions: vec![],
                    terminator: Terminator::Call {
                        callee: Callee::Direct(FnId(0)),
                        args: vec![
                            Operand::Copy(Place::local(Local(0))),
                            Operand::Copy(Place::local(Local(0))),
                        ],
                        dest: Place::local(Local(1)),
                        target: BlockId(1),
                        cancel: None,
                    },
                    terminator_span: zil_base::Span::default(),
                },
                Block {
                    instructions: vec![],
                    terminator: Terminator::Return {
                        value: Operand::Const(ConstValue::Unit),
                    },
                    terminator_span: zil_base::Span::default(),
                },
            ],
        },
    };
    let mod_b = Module {
        tys: tys_b,
        functions: vec![callee_apart, caller_aliasing],
        foreigns: vec![],
        statics: vec![],
        dyn_impls: vec![],
    };
    let bytes_b = encode_module(&mod_b);
    let dec_b = decode_module(&bytes_b).expect("wire decode forged apart module");
    let cert_b = zil_solve::solve_module(&dec_b, zil_solve::Budget { steps: 1_000_000 })
        .expect("solver produces valid borrow certificate for aliasing shared refs");
    let err_b =
        zil_check::check_module(dec_b, &cert_b, zil_check::CheckBudget { steps: 1_000_000 })
            .expect_err("checker must reject aliasing arguments to RequiresApart");
    assert_eq!(err_b.function, FnId(1));
    assert!(
        matches!(err_b.kind, zil_check::CheckErrorKind::CertificateShape(_)),
        "expected CertificateShape rejection, got {:?}",
        err_b.kind
    );
}

#[test]
fn projection_invalidation_and_history_rollback_execute_natively_on_dual_backends() {
    let dir = temp_dir("projection-invalidation-native");
    let src = r#"
extern "C" fn putchar(code: i32) -> i32;

enum ChanState { Init, Open, Closed }

struct Channel {
    state: ChanState,
    counter: i32,
    tag: i32,
}

fn open(ch: &mut Channel)
    contract {
        requires(state(ch, ChanState.Init));
        provides(state(ch, ChanState.Open));
        effects(invalidates(ch.state));
    }
{
    ch.state = ChanState.Open
}

fn send(ch: &mut Channel, byte: i32)
    contract {
        requires(state(ch, ChanState.Open));
        provides(state(ch, ChanState.Open));
        effects(invalidates());
    }
{
    unsafe { putchar(byte) }
}

fn bump_counter(ch: &mut Channel)
    contract {
        effects(invalidates(ch.counter));
    }
{
    ch.counter = ch.counter + 1
}

fn reset_all(ch: &mut Channel)
    contract {
        effects(invalidates(ch));
    }
{
    ch.counter = 0
    ch.tag = 42
}

fn reconnect(ch: &mut Channel)
    contract {
        requires(state(ch, ChanState.Open));
        provides(state(ch, ChanState.Open));
        effects(rollback(ch));
    }
{
    ch.state = ChanState.Closed
    ch.state = ChanState.Open
}

fn read_tag(ch: &Channel) -> i32
    contract {
        effects(invalidates());
    }
{
    return ch.tag
}

fn main() -> i32 {
    var ch = Channel { state: ChanState.Init, counter: 0, tag: 7 }
    open(&mut ch)
    // Projection-scoped invalidation on `ch.counter` preserves `ch.state == Open`!
    bump_counter(&mut ch)
    send(&mut ch, 65)
    // Explicit history rollback `effects(rollback(ch))` permits Open -> Closed -> Open.
    reconnect(&mut ch)
    send(&mut ch, 66)
    // Hierarchical upper bound `effects(invalidates(ch))` covers all subfields.
    reset_all(&mut ch)
    let t = read_tag(&ch)
    if t == 42 {
        return 0
    }
    return 1
}
"#;
    let entry = write_source(&dir, "main.zil", src);
    let (code, stdout) = build_and_run_dual_with_c(&dir, &entry, &[]);
    assert_eq!(code, 0);
    assert_eq!(stdout.as_slice(), b"AB");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn projection_invalidation_and_rollback_violations_are_rejected() {
    let dir = temp_dir("projection-invalidation-reject");

    // 1. Under-declared upper bound: declares invalidates(ch.counter) but mutates ch.tag.
    check_rejected(
        &dir,
        "under_declared_projection_invalidation_rejected",
        r#"
enum ChanState { Init, Open, Closed }
struct Channel { state: ChanState, counter: i32, tag: i32 }
fn bad(ch: &mut Channel) contract { effects(invalidates(ch.counter)); } {
    ch.tag = 99
}
fn main() -> i32 {
    var ch = Channel { state: ChanState.Init, counter: 0, tag: 0 }
    bad(&mut ch)
    return 0
}
"#,
        ResultCategory::SafeReject,
        "E-SAFETY-CHECK",
    );

    // 2. Empty upper bound effects(invalidates()) cannot mutate any parameter field.
    check_rejected(
        &dir,
        "empty_invalidation_upper_bound_rejects_mutation",
        r#"
struct Boxed { x: i32 }
fn bad(b: &mut Boxed) contract { effects(invalidates()); } {
    b.x = 1
}
fn main() -> i32 {
    var b = Boxed { x: 0 }
    bad(&mut b)
    return 0
}
"#,
        ResultCategory::SafeReject,
        "E-SAFETY-CHECK",
    );

    // 3. Hidden history rollback H (Open -> Closed -> Open) without effects(rollback(ch)).
    check_rejected(
        &dir,
        "hidden_history_rollback_rejected",
        r#"
enum ChanState { Init, Open, Closed }
struct Channel { state: ChanState }
fn sneaky_reset(ch: &mut Channel)
    contract {
        requires(state(ch, ChanState.Open));
        provides(state(ch, ChanState.Open));
    }
{
    ch.state = ChanState.Closed
    ch.state = ChanState.Open
}
fn main() -> i32 {
    var ch = Channel { state: ChanState.Open }
    sneaky_reset(&mut ch)
    return 0
}
"#,
        ResultCategory::SafeReject,
        "E-SAFETY-CHECK",
    );

    // 4. Calling an uncontracted mutable helper cannot satisfy empty effects(invalidates()).
    check_rejected(
        &dir,
        "uncontracted_callee_violates_empty_invalidation_upper_bound",
        r#"
struct Boxed { x: i32 }
fn unknown_mut(b: &mut Boxed) {
    b.x = 42
}
fn bad(b: &mut Boxed) contract { effects(invalidates()); } {
    unknown_mut(b)
}
fn main() -> i32 {
    var b = Boxed { x: 0 }
    bad(&mut b)
    return 0
}
"#,
        ResultCategory::SafeReject,
        "E-SAFETY-CHECK",
    );

    // 5. Unknown field name in effects(invalidates(ch.nonexistent)) rejected at typecheck.
    check_rejected(
        &dir,
        "unknown_field_in_invalidates_rejected",
        r#"
struct Boxed { x: i32 }
fn bad(b: &mut Boxed) contract { effects(invalidates(b.nonexistent)); } {}
fn main() -> i32 { return 0 }
"#,
        ResultCategory::SafeReject,
        "E-RESOURCE-RESULT-MODE",
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn raw_wire_forged_projection_invalidation_upper_bound_is_rejected_by_checker() {
    use zil_ir::body::{
        AdtDef, AdtKind, Block, Body, ConstValue, ContractFieldPath, FieldDef, FieldIdx, FieldRef,
        FnId, FnRole, FnSig, Function, GenericArgs, Instruction, InstructionKind, IntTy, Local,
        LocalDecl, Module, Mutability, Operand, ParamEnv, Place, PlaceRoot, Projection, Region,
        ResumeMode, Rvalue, StateContractClause, Terminator, TyKind, TyTable, VariantDef,
        VariantIdx, decode_module, encode_module,
    };

    let mut tys = TyTable::new();
    let unit_ty = tys.intern(TyKind::Unit);
    let i32_ty = tys.intern(TyKind::Int(IntTy::I32));
    let empty_args = tys.intern_args(GenericArgs {
        tys: vec![],
        regions: vec![],
    });
    let pair_adt = tys.add_adt(AdtDef {
        name: "Pair".into(),
        kind: AdtKind::Record,
        type_params: 0,
        region_params: 0,
        variants: vec![VariantDef {
            name: "Pair".into(),
            fields: vec![
                FieldDef {
                    name: "a".into(),
                    ty: i32_ty,
                },
                FieldDef {
                    name: "b".into(),
                    ty: i32_ty,
                },
            ],
        }],
        user_drop: false,
    });
    let pair_ty = tys.intern(TyKind::Adt {
        adt: pair_adt,
        args: empty_args,
    });
    let mut_pair = tys.intern(TyKind::Ref {
        region: Region::Static,
        mutability: Mutability::Exclusive,
        pointee: pair_ty,
    });

    // Function declares `effects(invalidates(p.a))` (FieldIdx(0)),
    // but body writes `(*p).b = 99` (FieldIdx(1)).
    let forged = Function {
        name: "forged_invalidation_bound".into(),
        sig: FnSig {
            params: vec![mut_pair],
            result: unit_ty,
            type_params: 0,
            region_params: 0,
            role: FnRole::Normal,
            resume_mode: ResumeMode::Local,
            state_contracts: vec![
                StateContractClause::InvalidationUpperBound,
                StateContractClause::Invalidates {
                    param: 0,
                    path: ContractFieldPath::from_slice(&[FieldIdx(0)]).unwrap(),
                },
            ],
        },
        body: Body {
            param_count: 1,
            locals: vec![LocalDecl {
                ty: mut_pair,
                name: Some("p".into()),
            }],
            captures: vec![],
            closure: None,
            result: unit_ty,
            param_env: ParamEnv::default(),
            drop_flags: vec![],
            blocks: vec![Block {
                instructions: vec![Instruction {
                    kind: InstructionKind::Assign {
                        dest: Place {
                            root: PlaceRoot::Local(Local(0)),
                            projections: vec![
                                Projection::Deref,
                                Projection::Field(FieldRef {
                                    adt: pair_adt,
                                    args: empty_args,
                                    variant: VariantIdx(0),
                                    field: FieldIdx(1),
                                }),
                            ],
                        },
                        value: Rvalue::Use(Operand::Const(ConstValue::Int {
                            ty: IntTy::I32,
                            bits: 99,
                        })),
                    },
                    span: zil_base::Span::default(),
                }],
                terminator: Terminator::Return {
                    value: Operand::Const(ConstValue::Unit),
                },
                terminator_span: zil_base::Span::default(),
            }],
        },
    };

    let module = Module {
        tys,
        functions: vec![forged],
        foreigns: vec![],
        statics: vec![],
        dyn_impls: vec![],
    };
    let bytes = encode_module(&module);
    let decoded = decode_module(&bytes).expect("wire decode forged invalidation module");
    let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
        .expect("solver produces valid borrow certificate");
    let err = zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 1_000_000 })
        .expect_err("checker must reject forged projection invalidation upper bound");
    assert_eq!(err.function, FnId(0));
    assert!(
        matches!(err.kind, zil_check::CheckErrorKind::CertificateShape(_)),
        "expected CertificateShape rejection, got {:?}",
        err.kind
    );
}

#[test]
fn owning_replace_and_branch_rollback_controls_execute_natively_on_dual_backends() {
    let dir = temp_dir("replace-and-rollback-native");
    let src = r#"
enum ChanState { Init, Open, Closed }
struct Channel { state: ChanState, code: i32 }
package Holder { manages payload: ByteBuffer; pub tag: i32 }

fn swap_payload(h: &mut Holder)
    contract { effects(invalidates(h.payload)); }
{
    let fresh = bytes_new()
    bytes_push(fresh, 42u8)
    h.payload = give fresh
}

fn branch_reconnect(ch: &mut Channel, do_reconnect: bool)
    contract {
        requires(state(ch, ChanState.Open));
        provides(state(ch, ChanState.Open, ChanState.Closed));
        effects(rollback(ch));
    }
{
    if do_reconnect {
        ch.state = ChanState.Closed
        ch.state = ChanState.Open
    } else {
        ch.state = ChanState.Closed
    }
}

fn main() -> i32 {
    var h = Holder { payload: bytes_new(), tag: 7 }
    swap_payload(&mut h)
    var ch = Channel { state: ChanState.Open, code: 1 }
    branch_reconnect(&mut ch, true)
    let b = bytes_view(h.payload)[0]
    if b == 42u8 && h.tag == 7 {
        return 0
    }
    return 1
}
"#;
    let entry = write_source(&dir, "main.zil", src);
    let (code, stdout) = build_and_run_dual_with_c(&dir, &entry, &[]);
    assert_eq!(code, 0);
    assert!(stdout.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn verifier_soundness_replace_cfg_alias_join_and_path_rollback_are_rejected() {
    let dir = temp_dir("verifier-soundness-reject");

    // P0-1a: Owning field Replace (`h.payload = bytes_new()`) rejected under empty invalidates().
    check_rejected(
        &dir,
        "replace_owning_field_violates_empty_invalidates",
        r#"
package Holder { manages payload: ByteBuffer; pub tag: i32 }
fn bad(h: &mut Holder) contract { effects(invalidates()); } {
    h.payload = bytes_new()
}
fn main() -> i32 {
    var h = Holder { payload: bytes_new(), tag: 0 }
    bad(&mut h)
    return 0
}
"#,
        ResultCategory::SafeReject,
        "E-SAFETY-CHECK",
    );

    // P0-1b: Owning field Replace on `h.payload` rejected when only `h.tag` is declared invalidated.
    check_rejected(
        &dir,
        "replace_owning_field_violates_disjoint_invalidates",
        r#"
package Holder { manages payload: ByteBuffer; pub tag: i32 }
fn bad(h: &mut Holder) contract { effects(invalidates(h.tag)); } {
    h.payload = bytes_new()
}
fn main() -> i32 {
    var h = Holder { payload: bytes_new(), tag: 0 }
    bad(&mut h)
    return 0
}
"#,
        ResultCategory::SafeReject,
        "E-SAFETY-CHECK",
    );

    // P0-2a: CFG join of `alias` (Open on then-branch, Closed on else-branch) must fail closed.
    check_rejected(
        &dir,
        "cfg_join_alias_open_then_closed_rejected",
        r#"
enum ChanState { Init, Open, Closed }
struct Channel { state: ChanState }
fn require_open(ch: &Channel) contract { requires(state(ch, ChanState.Open)); } {}
fn bad(flag: bool) {
    let a = Channel { state: ChanState.Open }
    let b = Channel { state: ChanState.Closed }
    var r = &a
    if flag {
        r = &a
    } else {
        r = &b
    }
    require_open(r)
}
fn main() -> i32 {
    bad(false)
    return 0
}
"#,
        ResultCategory::SafeReject,
        "E-SAFETY-CHECK",
    );

    // P0-2b: Reversed branch order (Closed on then-branch, Open on else-branch) must also fail closed.
    check_rejected(
        &dir,
        "cfg_join_alias_closed_then_open_rejected",
        r#"
enum ChanState { Init, Open, Closed }
struct Channel { state: ChanState }
fn require_open(ch: &Channel) contract { requires(state(ch, ChanState.Open)); } {}
fn bad(flag: bool) {
    let a = Channel { state: ChanState.Open }
    let b = Channel { state: ChanState.Closed }
    var r = &b
    if flag {
        r = &b
    } else {
        r = &a
    }
    require_open(r)
}
fn main() -> i32 {
    bad(true)
    return 0
}
"#,
        ResultCategory::SafeReject,
        "E-SAFETY-CHECK",
    );

    // P0-2c: CFG join of `param_alias` (p.a on then-branch, p.b on else-branch) must invalidate both.
    check_rejected(
        &dir,
        "cfg_join_param_alias_a_then_b_rejected",
        r#"
struct Pair { a: i32, b: i32 }
fn bad(p: &mut Pair, flag: bool) contract { effects(invalidates(p.a)); } {
    if flag {
        let r = &mut p.a
        *r = 1
    } else {
        let r = &mut p.b
        *r = 2
    }
}
fn main() -> i32 {
    var p = Pair { a: 0, b: 0 }
    bad(&mut p, false)
    return 0
}
"#,
        ResultCategory::SafeReject,
        "E-SAFETY-CHECK",
    );

    // P0-2d: Merged mutable ref to p.a vs p.b written after join (both branch orders).
    check_rejected(
        &dir,
        "cfg_join_merged_param_ref_a_then_b_rejected",
        r#"
struct Pair { a: i32, b: i32 }
fn bad(p: &mut Pair, flag: bool) contract { effects(invalidates(p.a)); } {
    var r = &mut p.a
    if flag {
        r = &mut p.a
    } else {
        r = &mut p.b
    }
    *r = 9
}
fn main() -> i32 {
    var p = Pair { a: 0, b: 0 }
    bad(&mut p, false)
    return 0
}
"#,
        ResultCategory::SafeReject,
        "E-SAFETY-CHECK",
    );

    check_rejected(
        &dir,
        "cfg_join_merged_param_ref_b_then_a_rejected",
        r#"
struct Pair { a: i32, b: i32 }
fn bad(p: &mut Pair, flag: bool) contract { effects(invalidates(p.a)); } {
    var r = &mut p.b
    if flag {
        r = &mut p.b
    } else {
        r = &mut p.a
    }
    *r = 9
}
fn main() -> i32 {
    var p = Pair { a: 0, b: 0 }
    bad(&mut p, true)
    return 0
}
"#,
        ResultCategory::SafeReject,
        "E-SAFETY-CHECK",
    );

    // P0-3a: Path-correlated rollback H: Branch A does Open->Closed->Open, Branch B does Open->Closed.
    check_rejected(
        &dir,
        "branch_rollback_hidden_by_other_branch_rejected",
        r#"
enum ChanState { Init, Open, Closed }
struct Channel { state: ChanState }
fn sneaky(ch: &mut Channel, flag: bool)
    contract {
        requires(state(ch, ChanState.Open));
        provides(state(ch, ChanState.Open, ChanState.Closed));
    }
{
    if flag {
        ch.state = ChanState.Closed
        ch.state = ChanState.Open
    } else {
        ch.state = ChanState.Closed
    }
}
fn main() -> i32 {
    var ch = Channel { state: ChanState.Open }
    sneaky(&mut ch, true)
    return 0
}
"#,
        ResultCategory::SafeReject,
        "E-SAFETY-CHECK",
    );

    // P0-3b: Reversed branch order (Branch A normal Open->Closed, Branch B rollback Open->Closed->Open).
    check_rejected(
        &dir,
        "branch_rollback_hidden_reversed_order_rejected",
        r#"
enum ChanState { Init, Open, Closed }
struct Channel { state: ChanState }
fn sneaky(ch: &mut Channel, flag: bool)
    contract {
        requires(state(ch, ChanState.Open));
        provides(state(ch, ChanState.Open, ChanState.Closed));
    }
{
    if flag {
        ch.state = ChanState.Closed
    } else {
        ch.state = ChanState.Closed
        ch.state = ChanState.Open
    }
}
fn main() -> i32 {
    var ch = Channel { state: ChanState.Open }
    sneaky(&mut ch, false)
    return 0
}
"#,
        ResultCategory::SafeReject,
        "E-SAFETY-CHECK",
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn raw_wire_replace_cfg_alias_join_and_branch_rollback_are_rejected_by_checker() {
    use zil_ir::body::{
        AdtDef, AdtKind, AggregateKind, Block, BlockId, Body, ConstValue, FieldDef, FieldIdx,
        FieldRef, FnId, FnRole, FnSig, Function, GenericArgs, Instruction, InstructionKind, IntTy,
        Local, LocalDecl, Module, Mutability, Operand, ParamEnv, Place, PlaceRoot, Projection,
        ProtocolStateSet, Region, ResumeMode, Rvalue, StateContractClause, Terminator, TyKind,
        TyTable, VariantDef, VariantIdx, decode_module, encode_module,
    };

    // Case 1 (P0-1 Raw/Wire): `InstructionKind::Replace` on `(*p).a` under empty `InvalidationUpperBound`.
    {
        let mut tys = TyTable::new();
        let unit_ty = tys.intern(TyKind::Unit);
        let i32_ty = tys.intern(TyKind::Int(IntTy::I32));
        let empty_args = tys.intern_args(GenericArgs {
            tys: vec![],
            regions: vec![],
        });
        let box_adt = tys.add_adt(AdtDef {
            name: "Boxed".into(),
            kind: AdtKind::Record,
            type_params: 0,
            region_params: 0,
            variants: vec![VariantDef {
                name: "Boxed".into(),
                fields: vec![FieldDef {
                    name: "a".into(),
                    ty: i32_ty,
                }],
            }],
            user_drop: false,
        });
        let box_ty = tys.intern(TyKind::Adt {
            adt: box_adt,
            args: empty_args,
        });
        let mut_box = tys.intern(TyKind::Ref {
            region: Region::Static,
            mutability: Mutability::Exclusive,
            pointee: box_ty,
        });
        let func = Function {
            name: "forged_replace".into(),
            sig: FnSig {
                params: vec![mut_box],
                result: unit_ty,
                type_params: 0,
                region_params: 0,
                role: FnRole::Normal,
                resume_mode: ResumeMode::Local,
                state_contracts: vec![StateContractClause::InvalidationUpperBound],
            },
            body: Body {
                param_count: 1,
                locals: vec![
                    LocalDecl {
                        ty: mut_box,
                        name: Some("p".into()),
                    },
                    LocalDecl {
                        ty: i32_ty,
                        name: Some("old".into()),
                    },
                ],
                captures: vec![],
                closure: None,
                result: unit_ty,
                param_env: ParamEnv::default(),
                drop_flags: vec![],
                blocks: vec![Block {
                    instructions: vec![Instruction {
                        kind: InstructionKind::Replace {
                            place: Place {
                                root: PlaceRoot::Local(Local(0)),
                                projections: vec![
                                    Projection::Deref,
                                    Projection::Field(FieldRef {
                                        adt: box_adt,
                                        args: empty_args,
                                        variant: VariantIdx(0),
                                        field: FieldIdx(0),
                                    }),
                                ],
                            },
                            value: Operand::Const(ConstValue::Int {
                                ty: IntTy::I32,
                                bits: 7,
                            }),
                            old: Local(1),
                        },
                        span: zil_base::Span::default(),
                    }],
                    terminator: Terminator::Return {
                        value: Operand::Const(ConstValue::Unit),
                    },
                    terminator_span: zil_base::Span::default(),
                }],
            },
        };
        let module = Module {
            tys,
            functions: vec![func],
            foreigns: vec![],
            statics: vec![],
            dyn_impls: vec![],
        };
        let bytes = encode_module(&module);
        let decoded = decode_module(&bytes).expect("wire decode Replace module");
        let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
            .expect("solver produces valid certificate for Replace");
        let err =
            zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 1_000_000 })
                .expect_err("checker must reject Replace under empty invalidation upper bound");
        assert_eq!(err.function, FnId(0));
        assert!(matches!(
            err.kind,
            zil_check::CheckErrorKind::CertificateShape(_)
        ));
    }

    // Case 2 (P0-3 Raw/Wire): Branch A does Open->Closed->Open, Branch B does Open->Closed,
    // merging to Return with RequiresState(Open) & ProvidesState(Open | Closed), NO AllowsRollback.
    {
        let mut tys = TyTable::new();
        let unit_ty = tys.intern(TyKind::Unit);
        let bool_ty = tys.intern(TyKind::Bool);
        let empty_args = tys.intern_args(GenericArgs {
            tys: vec![],
            regions: vec![],
        });
        let st_adt = tys.add_adt(AdtDef {
            name: "ChanState".into(),
            kind: AdtKind::Enum,
            type_params: 0,
            region_params: 0,
            variants: vec![
                VariantDef {
                    name: "Init".into(),
                    fields: vec![],
                },
                VariantDef {
                    name: "Open".into(),
                    fields: vec![],
                },
                VariantDef {
                    name: "Closed".into(),
                    fields: vec![],
                },
            ],
            user_drop: false,
        });
        let st_ty = tys.intern(TyKind::Adt {
            adt: st_adt,
            args: empty_args,
        });
        let ch_adt = tys.add_adt(AdtDef {
            name: "Channel".into(),
            kind: AdtKind::Record,
            type_params: 0,
            region_params: 0,
            variants: vec![VariantDef {
                name: "Channel".into(),
                fields: vec![FieldDef {
                    name: "state".into(),
                    ty: st_ty,
                }],
            }],
            user_drop: false,
        });
        let ch_ty = tys.intern(TyKind::Adt {
            adt: ch_adt,
            args: empty_args,
        });
        let mut_ch = tys.intern(TyKind::Ref {
            region: Region::Static,
            mutability: Mutability::Exclusive,
            pointee: ch_ty,
        });
        let state_place = || Place {
            root: PlaceRoot::Local(Local(0)),
            projections: vec![
                Projection::Deref,
                Projection::Field(FieldRef {
                    adt: ch_adt,
                    args: empty_args,
                    variant: VariantIdx(0),
                    field: FieldIdx(0),
                }),
            ],
        };
        let mk_variant = |v: u32| Rvalue::Aggregate {
            kind: AggregateKind::Adt {
                adt: st_adt,
                args: empty_args,
                variant: VariantIdx(v),
            },
            operands: vec![],
        };
        let func = Function {
            name: "forged_branch_rollback".into(),
            sig: FnSig {
                params: vec![mut_ch, bool_ty],
                result: unit_ty,
                type_params: 0,
                region_params: 0,
                role: FnRole::Normal,
                resume_mode: ResumeMode::Local,
                state_contracts: vec![
                    StateContractClause::RequiresState {
                        param: 0,
                        states: ProtocolStateSet::single(1),
                    },
                    StateContractClause::ProvidesState {
                        param: 0,
                        states: ProtocolStateSet::single(1).union(ProtocolStateSet::single(2)),
                    },
                ],
            },
            body: Body {
                param_count: 2,
                locals: vec![
                    LocalDecl {
                        ty: mut_ch,
                        name: Some("ch".into()),
                    },
                    LocalDecl {
                        ty: bool_ty,
                        name: Some("flag".into()),
                    },
                ],
                captures: vec![],
                closure: None,
                result: unit_ty,
                param_env: ParamEnv::default(),
                drop_flags: vec![],
                blocks: vec![
                    Block {
                        instructions: vec![],
                        terminator: Terminator::Switch {
                            discr: Operand::Copy(Place::local(Local(1))),
                            arms: vec![(1, BlockId(1))],
                            otherwise: BlockId(2),
                        },
                        terminator_span: zil_base::Span::default(),
                    },
                    // Branch A: Open -> Closed -> Open (hidden rollback!)
                    Block {
                        instructions: vec![
                            Instruction {
                                kind: InstructionKind::Assign {
                                    dest: state_place(),
                                    value: mk_variant(2),
                                },
                                span: zil_base::Span::default(),
                            },
                            Instruction {
                                kind: InstructionKind::Assign {
                                    dest: state_place(),
                                    value: mk_variant(1),
                                },
                                span: zil_base::Span::default(),
                            },
                        ],
                        terminator: Terminator::Goto { target: BlockId(3) },
                        terminator_span: zil_base::Span::default(),
                    },
                    // Branch B: Open -> Closed (normal)
                    Block {
                        instructions: vec![Instruction {
                            kind: InstructionKind::Assign {
                                dest: state_place(),
                                value: mk_variant(2),
                            },
                            span: zil_base::Span::default(),
                        }],
                        terminator: Terminator::Goto { target: BlockId(3) },
                        terminator_span: zil_base::Span::default(),
                    },
                    // Join + Return
                    Block {
                        instructions: vec![],
                        terminator: Terminator::Return {
                            value: Operand::Const(ConstValue::Unit),
                        },
                        terminator_span: zil_base::Span::default(),
                    },
                ],
            },
        };
        let module = Module {
            tys,
            functions: vec![func],
            foreigns: vec![],
            statics: vec![],
            dyn_impls: vec![],
        };
        let bytes = encode_module(&module);
        let decoded = decode_module(&bytes).expect("wire decode branch rollback module");
        let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
            .expect("solver produces valid certificate for branch rollback");
        let err =
            zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 1_000_000 })
                .expect_err(
                    "checker must reject path-correlated hidden rollback H across CFG merge",
                );
        assert_eq!(err.function, FnId(0));
        assert!(matches!(
            err.kind,
            zil_check::CheckErrorKind::CertificateShape(_)
        ));
    }
}

#[test]
fn unified_call_effects_and_apart_desc_execute_natively_on_dual_backends() {
    let dir = temp_dir("unified-call-effects-native");
    let src = r#"
struct Pair { a: i32, b: i32 }

trait Reader {
    fn read(self: &Self) -> i32;
}

impl Reader for Pair {
    fn read(self: &Self) -> i32 {
        return self.a + self.b
    }
}

fn read_via_closure(p: &Pair) -> i32
    contract { effects(invalidates()); }
{
    let f = () -> i32 => p.a + p.b
    return f()
}

fn read_via_dyn(r: &OwnedDyn<dyn Reader>) -> i32
    contract { effects(invalidates()); }
{
    return r.read()
}

fn push_apart_store(a: Store<i64>, b: Store<i64>) -> i64
    contract {
        requires(apart(a, b));
        effects(invalidates(a));
    }
{
    store_push(a, 7)
    return store_len(a) + store_len(b)
}

fn main() -> i32 {
    var p = Pair { a: 10, b: 32 }
    let c = read_via_closure(&p)
    var d1: i32 = 0
    match owned_dyn::<dyn Reader>(Pair { a: 10, b: 32 }) {
        .err(_) => { return 89 }
        .ok(owner) => {
            d1 = read_via_dyn(&owner)
        }
    }
    let s1: Store<i64> = store_new()
    let s2: Store<i64> = store_new()
    if !store_reserve(s1, 2) || !store_reserve(s2, 2) {
        return 90
    }
    let total = push_apart_store(s1, s2)
    if c == 42 && d1 == 42 && total == 1 {
        return 0
    }
    return 1
}
"#;
    let entry = write_source(&dir, "main.zil", src);
    let (code, stdout) = build_and_run_dual_with_c(&dir, &entry, &[]);
    assert_eq!(code, 0);
    assert!(stdout.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn unified_call_effects_closure_dynslot_and_desc_alias_are_rejected() {
    let dir = temp_dir("unified-call-effects-reject");

    // 1. Closure capturing `&mut Pair` mutating `p.a` rejected under `effects(invalidates())`.
    check_rejected(
        &dir,
        "closure_mut_capture_violates_empty_invalidates",
        r#"
struct Pair { a: i32, b: i32 }
fn bad(p: &mut Pair) contract { effects(invalidates()); } {
    var f = () -> i32 => {
        p.a = 1
        return p.a
    }
    let _ = f()
}
fn main() -> i32 {
    var p = Pair { a: 0, b: 0 }
    bad(&mut p)
    return 0
}
"#,
        ResultCategory::SafeReject,
        "E-SAFETY-CHECK",
    );

    // 2. Closure capturing `&mut Channel` hiding rollback Open->Closed->Open rejected.
    check_rejected(
        &dir,
        "closure_mut_capture_hides_protocol_rollback",
        r#"
enum ChanState { Init, Open, Closed }
struct Channel { state: ChanState }
fn sneaky(ch: &mut Channel)
    contract {
        requires(state(ch, ChanState.Open));
        provides(state(ch, ChanState.Open));
    }
{
    var f = () -> i32 => {
        ch.state = ChanState.Closed
        ch.state = ChanState.Open
        return 0
    }
    let _ = f()
}
fn main() -> i32 {
    var ch = Channel { state: ChanState.Open }
    sneaky(&mut ch)
    return 0
}
"#,
        ResultCategory::SafeReject,
        "E-SAFETY-CHECK",
    );

    // 3. Direct `&mut OwnedDyn<dyn Mutator>` slot call rejected under `effects(invalidates())`.
    check_rejected(
        &dir,
        "dynslot_mut_param_violates_empty_invalidates",
        r#"
struct Counter { n: i32 }
trait Mutator {
    fn bump(self: &mut Self) -> i32;
}
impl Mutator for Counter {
    fn bump(self: &mut Self) -> i32 {
        self.n = self.n + 1
        return self.n
    }
}
fn bad(m: &mut OwnedDyn<dyn Mutator>) contract { effects(invalidates()); } {
    let _ = m.bump()
}
fn main() -> i32 {
    match owned_dyn::<dyn Mutator>(Counter { n: 0 }) {
        .err(_) => { return 1 }
        .ok(c) => {
            var mc = c
            bad(&mut mc)
        }
    }
    return 0
}
"#,
        ResultCategory::SafeReject,
        "E-SAFETY-CHECK",
    );

    // 4. Borrowed provider `OwnedDyn<dyn Mutator, Context<'a>>` wrapping `p: &'a mut Pair`
    //    rejected under `effects(invalidates())`.
    check_rejected(
        &dir,
        "dynslot_borrowed_provider_mut_param_violates_empty_invalidates",
        r#"
struct Pair { a: i32, b: i32 }
struct Context<life a> { leaf: &'a mut Pair }
struct Provider<life a> { target: &'a mut Pair }
trait Mutator {
    fn bump(self: &mut Self) -> i32;
}
impl<life a> Mutator for Provider<'a> {
    fn bump(self: &mut Self) -> i32 {
        self.target.a = self.target.a + 1
        return self.target.a
    }
}
fn bad<life a>(p: &'a mut Pair) contract { effects(invalidates()); } {
    match owned_dyn::<dyn Mutator, Context<'a>>(Provider::<'a> { target: p }) {
        .err(_) => {}
        .ok(m) => {
            var mm = m
            let _ = mm.bump()
        }
    }
}
fn main() -> i32 {
    var p = Pair { a: 0, b: 0 }
    bad(&mut p)
    return 0
}
"#,
        ResultCategory::SafeReject,
        "E-SAFETY-CHECK",
    );

    // 5. Borrowed provider `OwnedDyn<dyn Mutator, Context<'a>>` wrapping `ch: &'a mut Channel`
    //    violates protocol state preservation / rollback.
    check_rejected(
        &dir,
        "dynslot_borrowed_provider_protocol_handle_violates_state_preservation",
        r#"
enum ChanState { Init, Open, Closed }
struct Channel { state: ChanState }
struct Context<life a> { leaf: &'a mut Channel }
struct ChanProvider<life a> { target: &'a mut Channel }
trait Mutator {
    fn flip(self: &mut Self) -> i32;
}
impl<life a> Mutator for ChanProvider<'a> {
    fn flip(self: &mut Self) -> i32 {
        self.target.state = ChanState.Closed
        self.target.state = ChanState.Open
        return 0
    }
}
fn bad<life a>(ch: &'a mut Channel)
    contract {
        requires(state(ch, ChanState.Open));
        provides(state(ch, ChanState.Open));
    }
{
    match owned_dyn::<dyn Mutator, Context<'a>>(ChanProvider::<'a> { target: ch }) {
        .err(_) => {}
        .ok(m) => {
            var mm = m
            let _ = mm.flip()
        }
    }
}
fn main() -> i32 {
    var ch = Channel { state: ChanState.Open }
    bad(&mut ch)
    return 0
}
"#,
        ResultCategory::SafeReject,
        "E-SAFETY-CHECK",
    );

    // 6. Cross-parameter `Desc` aliasing without `apart(a, b)`: mutating `a` also invalidates `b`.
    check_rejected(
        &dir,
        "cross_desc_alias_without_apart_violates_invalidates_a_only",
        r#"
fn bad(a: Store<i64>, b: Store<i64>)
    contract { effects(invalidates(a)); }
{
    store_push(a, 1)
}
fn main() -> i32 {
    let s: Store<i64> = store_new()
    if !store_reserve(s, 2) { return 1 }
    bad(s, s)
    return 0
}
"#,
        ResultCategory::SafeReject,
        "E-SAFETY-CHECK",
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn raw_wire_unified_call_effects_closure_dynslot_and_desc_alias_checked() {
    use zil_ir::body::{
        AdtDef, AdtKind, AggregateKind, Block, BlockId, Body, Callee, ClosureDef, ClosureKind,
        ConstValue, ContractFieldPath, DynDef, DynSlotSig, FieldDef, FieldIdx, FieldRef, FnId,
        FnRole, FnSig, Function, GenericArgs, Instruction, InstructionKind, IntTy, Local,
        LocalDecl, Module, Mutability, Operand, ParamEnv, Place, PlaceRoot, Projection,
        ProtocolStateSet, Region, ResumeMode, Rvalue, StateContractClause, Terminator, TyKind,
        TyTable, VariantDef, VariantIdx, decode_module, encode_module,
    };

    // --- Part A: Cross-parameter Desc aliasing (positive with RequiresApart, negative without,
    //     and negative hidden rollback through aliased Desc) ---
    let build_desc_alias_module = |with_apart: bool, do_rollback: bool| -> Module {
        let mut tys = TyTable::new();
        let unit_ty = tys.intern(TyKind::Unit);
        let empty_args = tys.intern_args(GenericArgs {
            tys: vec![],
            regions: vec![],
        });
        let st_adt = tys.add_adt(AdtDef {
            name: "ChanState".into(),
            kind: AdtKind::Enum,
            type_params: 0,
            region_params: 0,
            variants: vec![
                VariantDef {
                    name: "Init".into(),
                    fields: vec![],
                },
                VariantDef {
                    name: "Open".into(),
                    fields: vec![],
                },
                VariantDef {
                    name: "Closed".into(),
                    fields: vec![],
                },
            ],
            user_drop: false,
        });
        let st_ty = tys.intern(TyKind::Adt {
            adt: st_adt,
            args: empty_args,
        });
        let ch_adt = tys.add_adt(AdtDef {
            name: "Channel".into(),
            kind: AdtKind::Record,
            type_params: 0,
            region_params: 0,
            variants: vec![VariantDef {
                name: "Channel".into(),
                fields: vec![FieldDef {
                    name: "state".into(),
                    ty: st_ty,
                }],
            }],
            user_drop: false,
        });
        let ch_ty = tys.intern(TyKind::Adt {
            adt: ch_adt,
            args: empty_args,
        });
        let desc_ch = tys.intern(TyKind::Desc(ch_ty));
        let a_state = Place {
            root: PlaceRoot::Local(Local(0)),
            projections: vec![
                Projection::Deref,
                Projection::Field(FieldRef {
                    adt: ch_adt,
                    args: empty_args,
                    variant: VariantIdx(0),
                    field: FieldIdx(0),
                }),
            ],
        };
        let mk_variant = |v: u32| Rvalue::Aggregate {
            kind: AggregateKind::Adt {
                adt: st_adt,
                args: empty_args,
                variant: VariantIdx(v),
            },
            operands: vec![],
        };
        let mut contracts = vec![
            StateContractClause::RequiresState {
                param: 1,
                states: ProtocolStateSet::single(1),
            },
            StateContractClause::ProvidesState {
                param: 1,
                states: ProtocolStateSet::single(1),
            },
            StateContractClause::InvalidationUpperBound,
            StateContractClause::Invalidates {
                param: 0,
                path: ContractFieldPath::from_slice(&[FieldIdx(0)]).unwrap(),
            },
        ];
        if with_apart {
            contracts.push(StateContractClause::RequiresApart {
                param_a: 0,
                param_b: 1,
            });
        }
        let mut insts = vec![Instruction {
            kind: InstructionKind::Assign {
                dest: a_state.clone(),
                value: mk_variant(2),
            },
            span: zil_base::Span::default(),
        }];
        if do_rollback {
            // Allow invalidating both `a.state` and `b.state` so rejection specifically proves
            // hidden rollback detection on aliased `b`.
            contracts.push(StateContractClause::Invalidates {
                param: 1,
                path: ContractFieldPath::from_slice(&[FieldIdx(0)]).unwrap(),
            });
            insts.push(Instruction {
                kind: InstructionKind::Assign {
                    dest: a_state,
                    value: mk_variant(1),
                },
                span: zil_base::Span::default(),
            });
        }
        let func = Function {
            name: "desc_alias_probe".into(),
            sig: FnSig {
                params: vec![desc_ch, desc_ch],
                result: unit_ty,
                type_params: 0,
                region_params: 0,
                role: FnRole::Normal,
                resume_mode: ResumeMode::Local,
                state_contracts: contracts,
            },
            body: Body {
                param_count: 2,
                locals: vec![
                    LocalDecl {
                        ty: desc_ch,
                        name: Some("a".into()),
                    },
                    LocalDecl {
                        ty: desc_ch,
                        name: Some("b".into()),
                    },
                ],
                captures: vec![],
                closure: None,
                result: unit_ty,
                param_env: ParamEnv::default(),
                drop_flags: vec![],
                blocks: vec![Block {
                    instructions: insts,
                    terminator: Terminator::Return {
                        value: Operand::Const(ConstValue::Unit),
                    },
                    terminator_span: zil_base::Span::default(),
                }],
            },
        };
        Module {
            tys,
            functions: vec![func],
            foreigns: vec![],
            statics: vec![],
            dyn_impls: vec![],
        }
    };

    // A1. Positive: with RequiresApart(a, b), mutating a.state preserves b=Open.
    {
        let m = build_desc_alias_module(true, false);
        let decoded = decode_module(&encode_module(&m)).expect("decode apart desc module");
        let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
            .expect("solve apart desc module");
        zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 1_000_000 })
            .expect("checker must accept disjoint Desc pair with RequiresApart");
    }
    // A2. Negative: without RequiresApart(a, b), mutating a.state violates b=Open and invalidates(a.state).
    {
        let m = build_desc_alias_module(false, false);
        let decoded = decode_module(&encode_module(&m)).expect("decode aliased desc module");
        let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
            .expect("solve aliased desc module");
        let err =
            zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 1_000_000 })
                .expect_err("checker must reject aliased Desc mutation without RequiresApart");
        assert_eq!(err.function, FnId(0));
        assert!(matches!(
            err.kind,
            zil_check::CheckErrorKind::CertificateShape(_)
        ));
    }
    // A3. Negative: without RequiresApart(a, b), a.state = Closed; a.state = Open triggers rollback on b.
    {
        let m = build_desc_alias_module(false, true);
        let decoded = decode_module(&encode_module(&m)).expect("decode aliased desc rollback");
        let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
            .expect("solve aliased desc rollback");
        let err =
            zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 1_000_000 })
                .expect_err("checker must reject hidden rollback through aliased Desc parameter");
        assert_eq!(err.function, FnId(0));
        assert!(matches!(
            err.kind,
            zil_check::CheckErrorKind::CertificateShape(_)
        ));
    }

    // --- Part B: Closure capture effects (positive read-only vs negative mutable capture) ---
    let build_closure_module = |capture_mut: bool| -> Module {
        let mut tys = TyTable::new();
        let unit_ty = tys.intern(TyKind::Unit);
        let i32_ty = tys.intern(TyKind::Int(IntTy::I32));
        let empty_args = tys.intern_args(GenericArgs {
            tys: vec![],
            regions: vec![],
        });
        let cap_mut = if capture_mut {
            Mutability::Exclusive
        } else {
            Mutability::Shared
        };
        let ref_i32 = tys.intern(TyKind::Ref {
            region: Region::Static,
            mutability: cap_mut,
            pointee: i32_ty,
        });
        let clo_kind = if capture_mut {
            ClosureKind::FnMut
        } else {
            ClosureKind::Fn
        };
        let clo_def = tys.add_closure(ClosureDef {
            kind: clo_kind,
            type_params: 0,
            captures: vec![ref_i32],
            params: vec![],
            result: unit_ty,
        });
        let clo_ty = tys.intern(TyKind::Closure {
            def: clo_def,
            args: empty_args,
        });
        let caller = Function {
            name: "closure_effect_probe".into(),
            sig: FnSig {
                params: vec![ref_i32],
                result: unit_ty,
                type_params: 0,
                region_params: 0,
                role: FnRole::Normal,
                resume_mode: ResumeMode::Local,
                state_contracts: vec![StateContractClause::InvalidationUpperBound],
            },
            body: Body {
                param_count: 1,
                locals: vec![
                    LocalDecl {
                        ty: ref_i32,
                        name: Some("p".into()),
                    },
                    LocalDecl {
                        ty: clo_ty,
                        name: Some("cb".into()),
                    },
                    LocalDecl {
                        ty: unit_ty,
                        name: Some("res".into()),
                    },
                ],
                captures: vec![],
                closure: None,
                result: unit_ty,
                param_env: ParamEnv::default(),
                drop_flags: vec![],
                blocks: vec![
                    Block {
                        instructions: vec![Instruction {
                            kind: InstructionKind::Assign {
                                dest: Place::local(Local(1)),
                                value: Rvalue::Aggregate {
                                    kind: AggregateKind::Closure {
                                        def: clo_def,
                                        args: empty_args,
                                    },
                                    operands: vec![Operand::Move(Place::local(Local(0)))],
                                },
                            },
                            span: zil_base::Span::default(),
                        }],
                        terminator: Terminator::Call {
                            callee: Callee::Closure {
                                closure: Place::local(Local(1)),
                                mode: if capture_mut {
                                    zil_ir::body::ClosureCallMode::Mut
                                } else {
                                    zil_ir::body::ClosureCallMode::Shared
                                },
                            },
                            args: vec![],
                            dest: Place::local(Local(2)),
                            target: BlockId(1),
                            cancel: None,
                        },
                        terminator_span: zil_base::Span::default(),
                    },
                    Block {
                        instructions: vec![],
                        terminator: Terminator::Return {
                            value: Operand::Const(ConstValue::Unit),
                        },
                        terminator_span: zil_base::Span::default(),
                    },
                ],
            },
        };
        let clo_body = Function {
            name: "closure_body".into(),
            sig: FnSig {
                params: vec![],
                result: unit_ty,
                type_params: 0,
                region_params: 0,
                role: FnRole::Normal,
                resume_mode: ResumeMode::Local,
                state_contracts: vec![],
            },
            body: Body {
                param_count: 0,
                locals: vec![],
                captures: vec![ref_i32],
                closure: Some(clo_def),
                result: unit_ty,
                param_env: ParamEnv::default(),
                drop_flags: vec![],
                blocks: vec![Block {
                    instructions: vec![],
                    terminator: Terminator::Return {
                        value: Operand::Const(ConstValue::Unit),
                    },
                    terminator_span: zil_base::Span::default(),
                }],
            },
        };
        Module {
            tys,
            functions: vec![caller, clo_body],
            foreigns: vec![],
            statics: vec![],
            dyn_impls: vec![],
        }
    };

    // B1. Positive: read-only closure capturing `&i32` accepted under empty InvalidationUpperBound.
    {
        let m = build_closure_module(false);
        let decoded = decode_module(&encode_module(&m)).expect("decode readonly closure module");
        let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
            .expect("solve readonly closure module");
        zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 1_000_000 })
            .expect("checker must accept read-only closure under empty InvalidationUpperBound");
    }
    // B2. Negative: mutating closure capturing `&mut i32` rejected under empty InvalidationUpperBound.
    {
        let m = build_closure_module(true);
        let decoded = decode_module(&encode_module(&m)).expect("decode mut closure module");
        let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
            .expect("solve mut closure module");
        let err =
            zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 1_000_000 })
                .expect_err(
                    "checker must reject closure with &mut capture under empty upper bound",
                );
        assert_eq!(err.function, FnId(0));
        assert!(matches!(
            err.kind,
            zil_check::CheckErrorKind::CertificateShape(_)
        ));
    }

    // --- Part C: DynSlot receiver effects (positive pure &self vs negative &mut self) ---
    let build_dynslot_module = |slot_mut: bool| -> Module {
        let mut tys = TyTable::new();
        let unit_ty = tys.intern(TyKind::Unit);
        let i32_ty = tys.intern(TyKind::Int(IntTy::I32));
        let recv_mut = if slot_mut {
            Mutability::Exclusive
        } else {
            Mutability::Shared
        };
        let dyn_id = tys.add_dyn(DynDef {
            name: "Op".into(),
            slots: vec![DynSlotSig {
                name: "run".into(),
                receiver: Some(recv_mut),
                params: vec![],
                result: unit_ty,
            }],
        });
        let dyn_ty = tys.intern(TyKind::Dyn(dyn_id));
        let ref_dyn = tys.intern(TyKind::Ref {
            region: Region::Static,
            mutability: recv_mut,
            pointee: dyn_ty,
        });
        let ref_i32 = tys.intern(TyKind::Ref {
            region: Region::Static,
            mutability: recv_mut,
            pointee: i32_ty,
        });
        let func = Function {
            name: "dynslot_effect_probe".into(),
            sig: FnSig {
                params: vec![ref_dyn],
                result: unit_ty,
                type_params: 0,
                region_params: 0,
                role: FnRole::Normal,
                resume_mode: ResumeMode::Local,
                state_contracts: vec![StateContractClause::InvalidationUpperBound],
            },
            body: Body {
                param_count: 1,
                locals: vec![
                    LocalDecl {
                        ty: ref_dyn,
                        name: Some("obj".into()),
                    },
                    LocalDecl {
                        ty: unit_ty,
                        name: Some("res".into()),
                    },
                ],
                captures: vec![],
                closure: None,
                result: unit_ty,
                param_env: ParamEnv::default(),
                drop_flags: vec![],
                blocks: vec![
                    Block {
                        instructions: vec![],
                        terminator: Terminator::Call {
                            callee: Callee::DynSlot {
                                object: if slot_mut {
                                    Operand::Move(Place::local(Local(0)))
                                } else {
                                    Operand::Copy(Place::local(Local(0)))
                                },
                                slot: 0,
                            },
                            args: vec![],
                            dest: Place::local(Local(1)),
                            target: BlockId(1),
                            cancel: None,
                        },
                        terminator_span: zil_base::Span::default(),
                    },
                    Block {
                        instructions: vec![],
                        terminator: Terminator::Return {
                            value: Operand::Const(ConstValue::Unit),
                        },
                        terminator_span: zil_base::Span::default(),
                    },
                ],
            },
        };
        let slot_impl = Function {
            name: "op_run_i32".into(),
            sig: FnSig {
                params: vec![ref_i32],
                result: unit_ty,
                type_params: 0,
                region_params: 0,
                role: FnRole::DynSlotImpl {
                    dyn_id,
                    concrete_ty: i32_ty,
                    slot: 0,
                },
                resume_mode: ResumeMode::Local,
                state_contracts: vec![],
            },
            body: Body {
                param_count: 1,
                locals: vec![LocalDecl {
                    ty: ref_i32,
                    name: Some("self".into()),
                }],
                captures: vec![],
                closure: None,
                result: unit_ty,
                param_env: ParamEnv::default(),
                drop_flags: vec![],
                blocks: vec![Block {
                    instructions: vec![],
                    terminator: Terminator::Return {
                        value: Operand::Const(ConstValue::Unit),
                    },
                    terminator_span: zil_base::Span::default(),
                }],
            },
        };
        Module {
            tys,
            functions: vec![func, slot_impl],
            foreigns: vec![],
            statics: vec![],
            dyn_impls: vec![zil_ir::body::DynImpl {
                dyn_id,
                concrete_ty: i32_ty,
                methods: vec![FnId(1)],
            }],
        }
    };

    // C1. Positive: `&dyn Op` (`&self` slot) accepted under empty InvalidationUpperBound.
    {
        let m = build_dynslot_module(false);
        let decoded = decode_module(&encode_module(&m)).expect("decode shared dynslot module");
        let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
            .expect("solve shared dynslot module");
        zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 1_000_000 })
            .expect("checker must accept shared &dyn slot under empty InvalidationUpperBound");
    }
    // C2. Negative: `&mut dyn Op` (`&mut self` slot) rejected under empty InvalidationUpperBound.
    {
        let m = build_dynslot_module(true);
        let decoded = decode_module(&encode_module(&m)).expect("decode mut dynslot module");
        let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
            .expect("solve mut dynslot module");
        let err =
            zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 1_000_000 })
                .expect_err("checker must reject &mut dyn slot under empty InvalidationUpperBound");
        assert_eq!(err.function, FnId(0));
        assert!(matches!(
            err.kind,
            zil_check::CheckErrorKind::CertificateShape(_)
        ));
    }
}

#[test]
fn preconstructed_aggregate_mut_cap_controls_execute_natively_on_dual_backends() {
    let dir = temp_dir("entry-aggregate-mut-cap-native");
    let src = r#"
struct Pair { a: i32, b: i32 }
struct ReadContext<life a> { ptr: &'a Pair, tag: i32 }
struct MutContext<life a> { ptr: &'a mut Pair, tag: i32 }

fn inspect_readonly<life a>(rc: ReadContext<'a>) -> i32 {
    return rc.ptr.a + rc.tag
}

fn read_only_ok<life a>(rc: ReadContext<'a>) -> i32
    contract { effects(invalidates()); }
{
    return inspect_readonly(rc)
}

fn change<life a>(c: MutContext<'a>) {
    c.ptr.a = c.ptr.a + c.tag
}

fn precise_projection_ok<life a>(c: MutContext<'a>)
    contract { effects(invalidates(c.ptr)); }
{
    change(c)
}

fn main() -> i32 {
    var p = Pair { a: 10, b: 20 }
    let r = read_only_ok(ReadContext { ptr: &p, tag: 5 })
    precise_projection_ok(MutContext { ptr: &mut p, tag: 32 })
    if r == 15 && p.a == 42 && p.b == 20 {
        return 0
    }
    return 1
}
"#;
    let entry = write_source(&dir, "main.zil", src);
    let (code, stdout) = build_and_run_dual_with_c(&dir, &entry, &[]);
    assert_eq!(code, 0);
    assert!(stdout.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn preconstructed_aggregate_mut_cap_violations_are_rejected() {
    let dir = temp_dir("entry-aggregate-mut-cap-reject");

    // 1. Pre-constructed `Context<'a>` containing `&'a mut Pair` passed to unknown Direct `change(c)`
    //    MUST be rejected under `effects(invalidates())`.
    check_rejected(
        &dir,
        "preconstructed_context_direct_call_violates_empty_invalidates",
        r#"
struct Pair { a: i32 }
struct Context<life a> { ptr: &'a mut Pair, tag: i32 }
fn change<life a>(c: Context<'a>) { c.ptr.a = 1 }
fn bad<life a>(c: Context<'a>) contract { effects(invalidates()); } {
    change(c)
}
fn main() -> i32 {
    var p = Pair { a: 0 }
    bad(Context { ptr: &mut p, tag: 7 })
    return 0
}
"#,
        ResultCategory::SafeReject,
        "E-SAFETY-CHECK",
    );

    // 2. Disjoint field upper bound `effects(invalidates(c.tag))` must still reject mutating `c.ptr`.
    check_rejected(
        &dir,
        "preconstructed_context_direct_call_violates_disjoint_tag_invalidates",
        r#"
struct Pair { a: i32 }
struct Context<life a> { ptr: &'a mut Pair, tag: i32 }
fn change<life a>(c: Context<'a>) { c.ptr.a = 1 }
fn bad<life a>(c: Context<'a>) contract { effects(invalidates(c.tag)); } {
    change(c)
}
fn main() -> i32 {
    var p = Pair { a: 0 }
    bad(Context { ptr: &mut p, tag: 7 })
    return 0
}
"#,
        ResultCategory::SafeReject,
        "E-SAFETY-CHECK",
    );

    // 3. Pre-constructed `Context<'a>` captured mutably by a closure and modified
    //    MUST be rejected by independent `zil-check` under empty upper bound.
    check_rejected(
        &dir,
        "preconstructed_context_closure_capture_violates_empty_invalidates",
        r#"
struct Pair { a: i32 }
struct Context<life a> { ptr: &'a mut Pair, tag: i32 }
fn bad<life a>(c: Context<'a>) contract { effects(invalidates()); } {
    let p = c.ptr
    var f = () -> i32 => {
        p.a = 1
        return p.a
    }
    let _ = f()
}
fn main() -> i32 {
    var p = Pair { a: 0 }
    bad(Context { ptr: &mut p, tag: 7 })
    return 0
}
"#,
        ResultCategory::SafeReject,
        "E-SAFETY-CHECK",
    );

    // 4. Pre-constructed `Context<'a>` passed as Dyn provider input into `owned_dyn` MUST propagate
    //    mutation effect on `DynSlot` call and reject under `effects(invalidates())`.
    check_rejected(
        &dir,
        "preconstructed_context_dyn_provider_violates_empty_invalidates",
        r#"
struct Pair { a: i32 }
struct Context<life a> { ptr: &'a mut Pair, tag: i32 }
trait Mutator {
    fn bump(self: &mut Self) -> i32;
}
impl<life a> Mutator for Context<'a> {
    fn bump(self: &mut Self) -> i32 {
        self.ptr.a = self.ptr.a + 1
        return self.ptr.a
    }
}
fn bad<life a>(c: Context<'a>) contract { effects(invalidates()); } {
    match owned_dyn::<dyn Mutator, Context<'a>>(c) {
        .err(_) => {}
        .ok(m) => {
            var mm = m
            let _ = mm.bump()
        }
    }
}
fn main() -> i32 {
    var p = Pair { a: 0 }
    bad(Context { ptr: &mut p, tag: 7 })
    return 0
}
"#,
        ResultCategory::SafeReject,
        "E-SAFETY-CHECK",
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn raw_wire_preconstructed_aggregate_mut_cap_checked() {
    use zil_ir::body::{
        AdtDef, AdtKind, Block, BlockId, Body, Callee, ConstValue, ContractFieldPath, FieldDef,
        FieldIdx, FnId, FnRole, FnSig, Function, GenericArgs, IntTy, Local, LocalDecl, Module,
        Mutability, Operand, ParamEnv, Place, Region, ResumeMode, StateContractClause, Terminator,
        TyKind, TyTable, VariantDef, decode_module, encode_module,
    };

    let build_module = |declared_field: Option<u32>| -> Module {
        let mut tys = TyTable::new();
        let unit_ty = tys.intern(TyKind::Unit);
        let i32_ty = tys.intern(TyKind::Int(IntTy::I32));
        let empty_args = tys.intern_args(GenericArgs {
            tys: vec![],
            regions: vec![],
        });
        let mut_i32 = tys.intern(TyKind::Ref {
            region: Region::Static,
            mutability: Mutability::Exclusive,
            pointee: i32_ty,
        });
        let ctx_adt = tys.add_adt(AdtDef {
            name: "Context".into(),
            kind: AdtKind::Record,
            type_params: 0,
            region_params: 0,
            variants: vec![VariantDef {
                name: "Context".into(),
                fields: vec![
                    FieldDef {
                        name: "ptr".into(),
                        ty: mut_i32,
                    },
                    FieldDef {
                        name: "tag".into(),
                        ty: i32_ty,
                    },
                ],
            }],
            user_drop: false,
        });
        let ctx_ty = tys.intern(TyKind::Adt {
            adt: ctx_adt,
            args: empty_args,
        });
        let mut contracts = vec![StateContractClause::InvalidationUpperBound];
        if let Some(fidx) = declared_field {
            contracts.push(StateContractClause::Invalidates {
                param: 0,
                path: ContractFieldPath::from_slice(&[FieldIdx(fidx)]).unwrap(),
            });
        }
        let caller = Function {
            name: "context_caller".into(),
            sig: FnSig {
                params: vec![ctx_ty],
                result: unit_ty,
                type_params: 0,
                region_params: 0,
                role: FnRole::Normal,
                resume_mode: ResumeMode::Local,
                state_contracts: contracts,
            },
            body: Body {
                param_count: 1,
                locals: vec![
                    LocalDecl {
                        ty: ctx_ty,
                        name: Some("c".into()),
                    },
                    LocalDecl {
                        ty: unit_ty,
                        name: Some("res".into()),
                    },
                ],
                captures: vec![],
                closure: None,
                result: unit_ty,
                param_env: ParamEnv::default(),
                drop_flags: vec![],
                blocks: vec![
                    Block {
                        instructions: vec![],
                        terminator: Terminator::Call {
                            callee: Callee::Direct(FnId(1)),
                            args: vec![Operand::Move(Place::local(Local(0)))],
                            dest: Place::local(Local(1)),
                            target: BlockId(1),
                            cancel: None,
                        },
                        terminator_span: zil_base::Span::default(),
                    },
                    Block {
                        instructions: vec![],
                        terminator: Terminator::Return {
                            value: Operand::Const(ConstValue::Unit),
                        },
                        terminator_span: zil_base::Span::default(),
                    },
                ],
            },
        };
        let callee = Function {
            name: "uncontracted_change".into(),
            sig: FnSig {
                params: vec![ctx_ty],
                result: unit_ty,
                type_params: 0,
                region_params: 0,
                role: FnRole::Normal,
                resume_mode: ResumeMode::Local,
                state_contracts: vec![],
            },
            body: Body {
                param_count: 1,
                locals: vec![LocalDecl {
                    ty: ctx_ty,
                    name: Some("c".into()),
                }],
                captures: vec![],
                closure: None,
                result: unit_ty,
                param_env: ParamEnv::default(),
                drop_flags: vec![],
                blocks: vec![Block {
                    instructions: vec![],
                    terminator: Terminator::Return {
                        value: Operand::Const(ConstValue::Unit),
                    },
                    terminator_span: zil_base::Span::default(),
                }],
            },
        };
        Module {
            tys,
            functions: vec![caller, callee],
            foreigns: vec![],
            statics: vec![],
            dyn_impls: vec![],
        }
    };

    // 1. Forged empty upper bound MUST be rejected by independent zil-check.
    {
        let m = build_module(None);
        let decoded = decode_module(&encode_module(&m)).expect("decode empty upper ctx module");
        let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
            .expect("solve empty upper ctx module");
        let err =
            zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 1_000_000 })
                .expect_err("checker must reject pre-constructed Context under empty upper bound");
        assert_eq!(err.function, FnId(0));
        assert!(matches!(
            err.kind,
            zil_check::CheckErrorKind::CertificateShape(_)
        ));
    }

    // 2. Disjoint field `c.tag` (field 1) MUST be rejected because `c.ptr` (field 0) is invalidated.
    {
        let m = build_module(Some(1));
        let decoded = decode_module(&encode_module(&m)).expect("decode tag upper ctx module");
        let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
            .expect("solve tag upper ctx module");
        let err =
            zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 1_000_000 })
                .expect_err("checker must reject disjoint c.tag upper bound");
        assert_eq!(err.function, FnId(0));
        assert!(matches!(
            err.kind,
            zil_check::CheckErrorKind::CertificateShape(_)
        ));
    }

    // 3. Exact field `c.ptr` (field 0) MUST be accepted without widening to all of `c`.
    {
        let m = build_module(Some(0));
        let decoded = decode_module(&encode_module(&m)).expect("decode ptr upper ctx module");
        let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
            .expect("solve ptr upper ctx module");
        zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 1_000_000 })
            .expect("checker must accept exact c.ptr invalidation upper bound");
    }
}

#[test]
fn complete_type_graph_dyn_desc_and_readonly_controls_checked() {
    let dir = temp_dir("complete-type-graph-source");

    // 1. Negative Source: `&OwnedDyn<dyn Reader>` (`&self` slot) where a concrete provider manages
    //    `Store<i64>` and mutates it via descriptor inside `read(&self)` MUST be rejected by
    //    `zil-check` under `effects(invalidates())`.
    check_rejected(
        &dir,
        "shared_dyn_with_desc_provider_violates_empty_invalidates",
        r#"
package StoreWriter { manages target: Store<i64>; }

trait Reader {
    fn read(self: &Self) -> i64;
}

impl Reader for StoreWriter {
    fn read(self: &Self) -> i64 {
        store_push(self.target, 1)
        return store_len(self.target)
    }
}

fn bad(r: &OwnedDyn<dyn Reader>) -> i64
    contract { effects(invalidates()); }
{
    return r.read()
}

fn main() -> i32 {
    let s: Store<i64> = store_new()
    if !store_reserve(s, 2) { return 1 }
    match owned_dyn::<dyn Reader>(StoreWriter { target: give s }) {
        .err(_) => { return 2 }
        .ok(owner) => {
            let _ = bad(&owner)
        }
    }
    return 0
}
"#,
        ResultCategory::SafeReject,
        "E-SAFETY-CHECK",
    );

    // 2. Negative Source: shared reference `&DescBox` managing `Store<i64>` passed to
    //    an uncontracted `Direct` callee MUST be rejected under `effects(invalidates())`.
    check_rejected(
        &dir,
        "shared_ref_to_desc_box_violates_empty_invalidates",
        r#"
package DescBox { manages s: Store<i64>; pub tag: i32; }

fn touch_store(b: &DescBox) {
    store_push(b.s, 7)
}

fn bad(b: &DescBox)
    contract { effects(invalidates()); }
{
    touch_store(b)
}

fn main() -> i32 {
    let s: Store<i64> = store_new()
    if !store_reserve(s, 2) { return 1 }
    let box_val = DescBox { s: give s, tag: 3 }
    bad(&box_val)
    return 0
}
"#,
        ResultCategory::SafeReject,
        "E-SAFETY-CHECK",
    );

    // 3. Positive dual-backend O0/O2 control:
    //    - Pure `&OwnedDyn<dyn PureReader>` (all impls have no `Desc`/`&mut`/mutable `Managed`) under `effects(invalidates())`.
    //    - Exact projection `effects(invalidates(b.s))` on `&DescBox` (does not widen to `b.tag`).
    let ok_src = r#"
struct PureBox { x: i64 }
package DescBox { manages s: Store<i64>; pub tag: i32; }

trait PureReader {
    fn read(self: &Self) -> i64;
}

impl PureReader for PureBox {
    fn read(self: &Self) -> i64 {
        return self.x
    }
}

fn read_pure_dyn(r: &OwnedDyn<dyn PureReader>) -> i64
    contract { effects(invalidates()); }
{
    return r.read()
}

fn push_via_box(b: &DescBox) {
    store_push(b.s, 42)
}

fn precise_desc_box_projection(b: &DescBox)
    contract { effects(invalidates(b.s)); }
{
    push_via_box(b)
}

fn main() -> i32 {
    var got: i64 = 0
    match owned_dyn::<dyn PureReader>(PureBox { x: 10 }) {
        .err(_) => { return 90 }
        .ok(owner) => {
            got = read_pure_dyn(&owner)
        }
    }
    let s: Store<i64> = store_new()
    if !store_reserve(s, 2) { return 91 }
    let b = DescBox { s: give s, tag: 5 }
    precise_desc_box_projection(&b)
    if got == 10 && store_len(b.s) == 1 && b.tag == 5 {
        return 0
    }
    return 1
}
"#;
    let entry = write_source(&dir, "ok_main.zil", ok_src);
    let (code, stdout) = build_and_run_dual_with_c(&dir, &entry, &[]);
    assert_eq!(code, 0);
    assert!(stdout.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn raw_wire_complete_type_graph_param_dyn_and_opaque_checked() {
    use zil_ir::body::{
        AdtDef, AdtKind, Block, BlockId, Body, Callee, ConstValue, ContractFieldPath, DynDef,
        DynImpl, DynSlotSig, FieldDef, FnId, FnRole, FnSig, Function, GenericArgs, IntTy, Local,
        LocalDecl, Module, Mutability, OpaqueTy, Operand, ParamEnv, Place, Region, ResumeMode,
        StateContractClause, Terminator, TyKind, TyTable, VariantDef, decode_module, encode_module,
    };

    // --- 1. Raw/Wire: `&Param(0)` behind shared reference passed to indirect call ---
    {
        let build_param_module = |allow_invalidation: bool| -> Module {
            let mut tys = TyTable::new();
            let unit_ty = tys.intern(TyKind::Unit);
            let t_param = tys.intern(TyKind::Param(0));
            let shared_t = tys.intern(TyKind::Ref {
                region: Region::Static,
                mutability: Mutability::Shared,
                pointee: t_param,
            });
            let fn_ptr_ty = tys.intern(TyKind::FnPtr {
                params: vec![shared_t],
                result: unit_ty,
            });
            let mut contracts = vec![StateContractClause::InvalidationUpperBound];
            if allow_invalidation {
                contracts.push(StateContractClause::Invalidates {
                    param: 0,
                    path: ContractFieldPath::from_slice(&[]).unwrap(),
                });
            }
            let caller = Function {
                name: "generic_shared_caller".into(),
                sig: FnSig {
                    params: vec![shared_t, fn_ptr_ty],
                    result: unit_ty,
                    type_params: 1,
                    region_params: 0,
                    role: FnRole::Normal,
                    resume_mode: ResumeMode::Local,
                    state_contracts: contracts,
                },
                body: Body {
                    param_count: 2,
                    locals: vec![
                        LocalDecl {
                            ty: shared_t,
                            name: Some("v".into()),
                        },
                        LocalDecl {
                            ty: fn_ptr_ty,
                            name: Some("f".into()),
                        },
                        LocalDecl {
                            ty: unit_ty,
                            name: Some("res".into()),
                        },
                    ],
                    captures: vec![],
                    closure: None,
                    result: unit_ty,
                    param_env: ParamEnv::default(),
                    drop_flags: vec![],
                    blocks: vec![
                        Block {
                            instructions: vec![],
                            terminator: Terminator::Call {
                                callee: Callee::Indirect(Operand::Copy(Place::local(Local(1)))),
                                args: vec![Operand::Copy(Place::local(Local(0)))],
                                dest: Place::local(Local(2)),
                                target: BlockId(1),
                                cancel: None,
                            },
                            terminator_span: zil_base::Span::default(),
                        },
                        Block {
                            instructions: vec![],
                            terminator: Terminator::Return {
                                value: Operand::Const(ConstValue::Unit),
                            },
                            terminator_span: zil_base::Span::default(),
                        },
                    ],
                },
            };
            Module {
                tys,
                functions: vec![caller],
                foreigns: vec![],
                statics: vec![],
                dyn_impls: vec![],
            }
        };

        // 1a. Negative: `&Param(0)` under empty upper bound MUST be rejected (`CertificateShape`).
        let m_bad = build_param_module(false);
        let decoded = decode_module(&encode_module(&m_bad)).expect("decode &Param bad module");
        let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
            .expect("solve &Param bad module");
        let err =
            zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 1_000_000 })
                .expect_err("checker must reject &Param(0) under empty invalidation upper bound");
        assert_eq!(err.function, FnId(0));
        assert!(matches!(
            err.kind,
            zil_check::CheckErrorKind::CertificateShape(_)
        ));

        // 1b. Positive: `&Param(0)` with declared `Invalidates(0, [])` MUST be accepted.
        let m_ok = build_param_module(true);
        let decoded = decode_module(&encode_module(&m_ok)).expect("decode &Param ok module");
        let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
            .expect("solve &Param ok module");
        zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 1_000_000 })
            .expect("checker must accept &Param(0) when invalidation is declared");
    }

    // --- 2. Raw/Wire: `&dyn Trait` + `&self` with Desc-carrying provider or empty impls ---
    {
        #[derive(Clone, Copy)]
        enum DynImplCase {
            DescProvider,
            EmptyImpls,
            PureProvider,
        }
        let build_shared_dyn_module = |case: DynImplCase| -> Module {
            let mut tys = TyTable::new();
            let unit_ty = tys.intern(TyKind::Unit);
            let i64_ty = tys.intern(TyKind::Int(IntTy::I64));
            let empty_args = tys.intern_args(GenericArgs {
                tys: vec![],
                regions: vec![],
            });
            let store_ty = tys.intern(TyKind::Opaque(OpaqueTy::OwnedStore { element: i64_ty }));
            let desc_store = tys.intern(TyKind::Desc(store_ty));
            let provider_adt = tys.add_adt(AdtDef {
                name: "Provider".into(),
                kind: AdtKind::Record,
                type_params: 0,
                region_params: 0,
                variants: vec![VariantDef {
                    name: "Provider".into(),
                    fields: vec![FieldDef {
                        name: "payload".into(),
                        ty: match case {
                            DynImplCase::DescProvider => desc_store,
                            DynImplCase::EmptyImpls | DynImplCase::PureProvider => i64_ty,
                        },
                    }],
                }],
                user_drop: false,
            });
            let provider_ty = tys.intern(TyKind::Adt {
                adt: provider_adt,
                args: empty_args,
            });
            let ref_provider = tys.intern(TyKind::Ref {
                region: Region::Static,
                mutability: Mutability::Shared,
                pointee: provider_ty,
            });
            let dyn_id = tys.add_dyn(DynDef {
                name: "Act".into(),
                slots: vec![DynSlotSig {
                    name: "run".into(),
                    receiver: Some(Mutability::Shared),
                    params: vec![],
                    result: unit_ty,
                }],
            });
            let dyn_ty = tys.intern(TyKind::Dyn(dyn_id));
            let ref_dyn = tys.intern(TyKind::Ref {
                region: Region::Static,
                mutability: Mutability::Shared,
                pointee: dyn_ty,
            });
            let caller = Function {
                name: "shared_dyn_caller".into(),
                sig: FnSig {
                    params: vec![ref_dyn],
                    result: unit_ty,
                    type_params: 0,
                    region_params: 0,
                    role: FnRole::Normal,
                    resume_mode: ResumeMode::Local,
                    state_contracts: vec![StateContractClause::InvalidationUpperBound],
                },
                body: Body {
                    param_count: 1,
                    locals: vec![
                        LocalDecl {
                            ty: ref_dyn,
                            name: Some("obj".into()),
                        },
                        LocalDecl {
                            ty: unit_ty,
                            name: Some("res".into()),
                        },
                    ],
                    captures: vec![],
                    closure: None,
                    result: unit_ty,
                    param_env: ParamEnv::default(),
                    drop_flags: vec![],
                    blocks: vec![
                        Block {
                            instructions: vec![],
                            terminator: Terminator::Call {
                                callee: Callee::DynSlot {
                                    object: Operand::Copy(Place::local(Local(0))),
                                    slot: 0,
                                },
                                args: vec![],
                                dest: Place::local(Local(1)),
                                target: BlockId(1),
                                cancel: None,
                            },
                            terminator_span: zil_base::Span::default(),
                        },
                        Block {
                            instructions: vec![],
                            terminator: Terminator::Return {
                                value: Operand::Const(ConstValue::Unit),
                            },
                            terminator_span: zil_base::Span::default(),
                        },
                    ],
                },
            };
            let mut functions = vec![caller];
            let mut dyn_impls = Vec::new();
            if !matches!(case, DynImplCase::EmptyImpls) {
                functions.push(Function {
                    name: "act_run_provider".into(),
                    sig: FnSig {
                        params: vec![ref_provider],
                        result: unit_ty,
                        type_params: 0,
                        region_params: 0,
                        role: FnRole::DynSlotImpl {
                            dyn_id,
                            concrete_ty: provider_ty,
                            slot: 0,
                        },
                        resume_mode: ResumeMode::Local,
                        state_contracts: vec![],
                    },
                    body: Body {
                        param_count: 1,
                        locals: vec![LocalDecl {
                            ty: ref_provider,
                            name: Some("self".into()),
                        }],
                        captures: vec![],
                        closure: None,
                        result: unit_ty,
                        param_env: ParamEnv::default(),
                        drop_flags: vec![],
                        blocks: vec![Block {
                            instructions: vec![],
                            terminator: Terminator::Return {
                                value: Operand::Const(ConstValue::Unit),
                            },
                            terminator_span: zil_base::Span::default(),
                        }],
                    },
                });
                dyn_impls.push(DynImpl {
                    dyn_id,
                    concrete_ty: provider_ty,
                    methods: vec![FnId(1)],
                });
            }
            Module {
                tys,
                functions,
                foreigns: vec![],
                statics: vec![],
                dyn_impls,
            }
        };

        // 2a. Negative: `&dyn Act` (&self) with Desc-carrying Provider MUST be rejected.
        let m_desc = build_shared_dyn_module(DynImplCase::DescProvider);
        let decoded = decode_module(&encode_module(&m_desc)).expect("decode dyn desc module");
        let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
            .expect("solve dyn desc module");
        let err =
            zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 1_000_000 })
                .expect_err("checker must reject &dyn (&self) when provider holds Desc");
        assert_eq!(err.function, FnId(0));
        assert!(matches!(
            err.kind,
            zil_check::CheckErrorKind::CertificateShape(_)
        ));

        // 2b. Negative: `&dyn Act` (&self) with empty dyn_impls MUST fail closed.
        let m_empty = build_shared_dyn_module(DynImplCase::EmptyImpls);
        let decoded = decode_module(&encode_module(&m_empty)).expect("decode dyn empty module");
        let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
            .expect("solve dyn empty module");
        let err =
            zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 1_000_000 })
                .expect_err("checker must reject &dyn (&self) when no concrete impls are known");
        assert_eq!(err.function, FnId(0));
        assert!(matches!(
            err.kind,
            zil_check::CheckErrorKind::CertificateShape(_)
        ));

        // 2c. Positive: `&dyn Act` (&self) with verifiably pure Provider MUST be accepted.
        let m_pure = build_shared_dyn_module(DynImplCase::PureProvider);
        let decoded = decode_module(&encode_module(&m_pure)).expect("decode dyn pure module");
        let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
            .expect("solve dyn pure module");
        zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 1_000_000 })
            .expect("checker must accept &dyn (&self) when all concrete impls are pure");
    }

    // --- 3. Raw/Wire: Opaque payloads (`Task<Unit, Context<&mut>>` & `&Mutex<Context<Desc>>`) ---
    {
        #[derive(Clone, Copy)]
        enum OpaqueCase {
            TaskWithMutContext,
            MutexWithDescContext,
            MutexPureI64,
        }
        let build_opaque_module = |case: OpaqueCase| -> Module {
            let mut tys = TyTable::new();
            let unit_ty = tys.intern(TyKind::Unit);
            let i64_ty = tys.intern(TyKind::Int(IntTy::I64));
            let empty_args = tys.intern_args(GenericArgs {
                tys: vec![],
                regions: vec![],
            });
            let mut_i64 = tys.intern(TyKind::Ref {
                region: Region::Static,
                mutability: Mutability::Exclusive,
                pointee: i64_ty,
            });
            let store_ty = tys.intern(TyKind::Opaque(OpaqueTy::OwnedStore { element: i64_ty }));
            let desc_store = tys.intern(TyKind::Desc(store_ty));
            let param_ty = match case {
                OpaqueCase::TaskWithMutContext => {
                    let ctx_adt = tys.add_adt(AdtDef {
                        name: "TaskCtx".into(),
                        kind: AdtKind::Record,
                        type_params: 0,
                        region_params: 0,
                        variants: vec![VariantDef {
                            name: "TaskCtx".into(),
                            fields: vec![FieldDef {
                                name: "ptr".into(),
                                ty: mut_i64,
                            }],
                        }],
                        user_drop: false,
                    });
                    let ctx_ty = tys.intern(TyKind::Adt {
                        adt: ctx_adt,
                        args: empty_args,
                    });
                    let task_ty = tys.intern(TyKind::Opaque(OpaqueTy::Task {
                        result: unit_ty,
                        context: Some(ctx_ty),
                    }));
                    tys.intern(TyKind::Ref {
                        region: Region::Static,
                        mutability: Mutability::Shared,
                        pointee: task_ty,
                    })
                }
                OpaqueCase::MutexWithDescContext => {
                    let ctx_adt = tys.add_adt(AdtDef {
                        name: "MutexCtx".into(),
                        kind: AdtKind::Record,
                        type_params: 0,
                        region_params: 0,
                        variants: vec![VariantDef {
                            name: "MutexCtx".into(),
                            fields: vec![FieldDef {
                                name: "s".into(),
                                ty: desc_store,
                            }],
                        }],
                        user_drop: false,
                    });
                    let ctx_ty = tys.intern(TyKind::Adt {
                        adt: ctx_adt,
                        args: empty_args,
                    });
                    let mutex_ty = tys.intern(TyKind::Opaque(OpaqueTy::Mutex { payload: ctx_ty }));
                    tys.intern(TyKind::Ref {
                        region: Region::Static,
                        mutability: Mutability::Shared,
                        pointee: mutex_ty,
                    })
                }
                OpaqueCase::MutexPureI64 => {
                    let mutex_ty = tys.intern(TyKind::Opaque(OpaqueTy::Mutex { payload: i64_ty }));
                    tys.intern(TyKind::Ref {
                        region: Region::Static,
                        mutability: Mutability::Shared,
                        pointee: mutex_ty,
                    })
                }
            };
            let caller = Function {
                name: "opaque_payload_caller".into(),
                sig: FnSig {
                    params: vec![param_ty],
                    result: unit_ty,
                    type_params: 0,
                    region_params: 0,
                    role: FnRole::Normal,
                    resume_mode: ResumeMode::Local,
                    state_contracts: vec![StateContractClause::InvalidationUpperBound],
                },
                body: Body {
                    param_count: 1,
                    locals: vec![
                        LocalDecl {
                            ty: param_ty,
                            name: Some("h".into()),
                        },
                        LocalDecl {
                            ty: unit_ty,
                            name: Some("res".into()),
                        },
                    ],
                    captures: vec![],
                    closure: None,
                    result: unit_ty,
                    param_env: ParamEnv::default(),
                    drop_flags: vec![],
                    blocks: vec![
                        Block {
                            instructions: vec![],
                            terminator: Terminator::Call {
                                callee: Callee::Direct(FnId(1)),
                                args: vec![Operand::Copy(Place::local(Local(0)))],
                                dest: Place::local(Local(1)),
                                target: BlockId(1),
                                cancel: None,
                            },
                            terminator_span: zil_base::Span::default(),
                        },
                        Block {
                            instructions: vec![],
                            terminator: Terminator::Return {
                                value: Operand::Const(ConstValue::Unit),
                            },
                            terminator_span: zil_base::Span::default(),
                        },
                    ],
                },
            };
            let callee = Function {
                name: "unknown_opaque_callee".into(),
                sig: FnSig {
                    params: vec![param_ty],
                    result: unit_ty,
                    type_params: 0,
                    region_params: 0,
                    role: FnRole::Normal,
                    resume_mode: ResumeMode::Local,
                    state_contracts: vec![],
                },
                body: Body {
                    param_count: 1,
                    locals: vec![LocalDecl {
                        ty: param_ty,
                        name: Some("h".into()),
                    }],
                    captures: vec![],
                    closure: None,
                    result: unit_ty,
                    param_env: ParamEnv::default(),
                    drop_flags: vec![],
                    blocks: vec![Block {
                        instructions: vec![],
                        terminator: Terminator::Return {
                            value: Operand::Const(ConstValue::Unit),
                        },
                        terminator_span: zil_base::Span::default(),
                    }],
                },
            };
            Module {
                tys,
                functions: vec![caller, callee],
                foreigns: vec![],
                statics: vec![],
                dyn_impls: vec![],
            }
        };

        // 3a. Negative: `&Task<Unit, Context<&mut>>` under empty upper bound MUST be rejected.
        let m_task = build_opaque_module(OpaqueCase::TaskWithMutContext);
        let decoded = decode_module(&encode_module(&m_task)).expect("decode Task ctx module");
        let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
            .expect("solve Task ctx module");
        let err =
            zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 1_000_000 })
                .expect_err("checker must reject Task carrying &mut context under empty bound");
        assert_eq!(err.function, FnId(0));
        assert!(matches!(
            err.kind,
            zil_check::CheckErrorKind::CertificateShape(_)
        ));

        // 3b. Negative: `&Mutex<Context<Desc>>` under empty upper bound MUST be rejected.
        let m_mutex = build_opaque_module(OpaqueCase::MutexWithDescContext);
        let decoded = decode_module(&encode_module(&m_mutex)).expect("decode Mutex desc module");
        let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
            .expect("solve Mutex desc module");
        let err =
            zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 1_000_000 })
                .expect_err("checker must reject Mutex carrying Desc payload under empty bound");
        assert_eq!(err.function, FnId(0));
        assert!(matches!(
            err.kind,
            zil_check::CheckErrorKind::CertificateShape(_)
        ));

        // 3c. Positive: `&Mutex<i64>` (no external Desc/&mut) accepted under empty upper bound.
        let m_pure_mutex = build_opaque_module(OpaqueCase::MutexPureI64);
        let decoded =
            decode_module(&encode_module(&m_pure_mutex)).expect("decode pure Mutex module");
        let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
            .expect("solve pure Mutex module");
        zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 1_000_000 })
            .expect("checker must accept pure Mutex<i64> read-only control");
    }
}

#[test]
fn managed_describe_protocol_and_projection_controls_execute_natively_on_dual_backends() {
    let dir = temp_dir("managed-describe-native");
    let src = r#"
enum ChanState { Open, Closed }
resource Channel { state: ChanState }
package Context { manages handle: Channel; pub tag: i32; }

fn flip_via_desc(ch: &Managed<Channel>) {
    let d: Channel = ch
    d.state = ChanState.Closed
    d.state = ChanState.Open
}

fn allowed_rollback(ch: &Managed<Channel>)
    contract {
        requires(state(ch, ChanState.Open));
        provides(state(ch, ChanState.Open, ChanState.Closed));
        effects(rollback(ch));
    }
{
    flip_via_desc(ch)
}

fn mutate_context_handle(c: &Context) {
    let d: Channel = c.handle
    d.state = ChanState.Closed
}

fn precise_handle_projection(c: &Context)
    contract { effects(invalidates(c.handle)); }
{
    mutate_context_handle(c)
}

fn main() -> i32 {
    let ch = Channel { state: ChanState.Open }
    allowed_rollback(&ch)
    let ctx = Context { handle: give ch, tag: 7 }
    precise_handle_projection(&ctx)
    match ctx.handle.state {
        ChanState.Closed() => {
            if ctx.tag == 7 {
                return 0
            }
        }
        ChanState.Open() => {}
    }
    return 1
}
"#;
    let entry = write_source(&dir, "main.zil", src);
    let (code, stdout) = build_and_run_dual_with_c(&dir, &entry, &[]);
    assert_eq!(code, 0);
    assert!(stdout.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn managed_describe_protocol_dyn_and_projection_violations_are_rejected() {
    let dir = temp_dir("managed-describe-reject");

    // 1. `&Managed<Channel>` passed to uncontracted Direct `change(ch)` violates `provides(state(ch, Open))`.
    check_rejected_with_reason(
        &dir,
        "managed_shared_ref_direct_call_violates_state_preservation",
        r#"
enum ChanState { Open, Closed }
resource Channel { state: ChanState }
fn change(ch: &Managed<Channel>) {
    let d: Channel = ch
    d.state = ChanState.Closed
}
fn bad(ch: &Managed<Channel>)
    contract {
        requires(state(ch, ChanState.Open));
        provides(state(ch, ChanState.Open));
    }
{
    change(ch)
}
fn main() -> i32 {
    let ch = Channel { state: ChanState.Open }
    bad(&ch)
    return 0
}
"#,
        "E-SAFETY-CHECK",
        "provides(state(...))",
    );

    // 2. `&Managed<Channel>` Open -> Closed -> Open via uncontracted Direct call triggers `H` rollback rejection
    //    even when `provides(state(ch, Open, Closed))` allows both states.
    check_rejected_with_reason(
        &dir,
        "managed_shared_ref_direct_call_triggers_history_rollback_h",
        r#"
enum ChanState { Open, Closed }
resource Channel { state: ChanState }
fn bounce(ch: &Managed<Channel>) {
    let d: Channel = ch
    d.state = ChanState.Closed
    d.state = ChanState.Open
}
fn bad(ch: &Managed<Channel>)
    contract {
        requires(state(ch, ChanState.Open));
        provides(state(ch, ChanState.Open, ChanState.Closed));
    }
{
    bounce(ch)
}
fn main() -> i32 {
    let ch = Channel { state: ChanState.Open }
    bad(&ch)
    return 0
}
"#,
        "E-SAFETY-CHECK",
        "rollback",
    );

    // 3. `Provider { manages channel: Channel }` via `&self` `DynSlot` mutating state MUST be rejected
    //    under `effects(invalidates())`.
    check_rejected_with_reason(
        &dir,
        "managed_dyn_provider_shared_slot_violates_empty_invalidates",
        r#"
enum ChanState { Open, Closed }
resource Channel { state: ChanState }
package Provider { manages channel: Channel; }
trait Inspector {
    fn inspect(self: &Self) -> i32;
}
impl Inspector for Provider {
    fn inspect(self: &Self) -> i32 {
        let d: Channel = self.channel
        d.state = ChanState.Closed
        return 1
    }
}
fn bad(r: &OwnedDyn<dyn Inspector>) -> i32
    contract { effects(invalidates()); }
{
    return r.inspect()
}
fn main() -> i32 {
    let ch = Channel { state: ChanState.Open }
    match owned_dyn::<dyn Inspector>(Provider { channel: give ch }) {
        .err(_) => { return 1 }
        .ok(owner) => {
            let _ = bad(&owner)
        }
    }
    return 0
}
"#,
        "E-SAFETY-CHECK",
        "invalidates",
    );

    // 4. Borrowed provider wrapping `ch: &'a Managed<Channel>` via `&self` `DynSlot` MUST invalidate
    //    `ch`'s protocol state and trigger rejection under `provides(state(ch, Open))`.
    check_rejected_with_reason(
        &dir,
        "managed_borrowed_dyn_provider_shared_slot_violates_protocol_state",
        r#"
enum ChanState { Open, Closed }
resource Channel { state: ChanState }
struct Context<life a> { leaf: &'a Managed<Channel> }
struct ChanProvider<life a> { channel: &'a Managed<Channel> }
trait Inspector {
    fn inspect(self: &Self) -> i32;
}
impl<life a> Inspector for ChanProvider<'a> {
    fn inspect(self: &Self) -> i32 {
        let d: Channel = self.channel
        d.state = ChanState.Closed
        return 1
    }
}
fn bad<life a>(ch: &'a Managed<Channel>)
    contract {
        requires(state(ch, ChanState.Open));
        provides(state(ch, ChanState.Open));
    }
{
    match owned_dyn::<dyn Inspector, Context<'a>>(ChanProvider::<'a> { channel: ch }) {
        .err(_) => {}
        .ok(owner) => {
            let _ = owner.inspect()
        }
    }
}
fn main() -> i32 {
    let ch = Channel { state: ChanState.Open }
    bad(&ch)
    return 0
}
"#,
        "E-SAFETY-CHECK",
        "provides(state(...))",
    );

    // 5. `Context { manages handle: Channel; pub tag: i32 }` under `effects(invalidates())`
    //    and under disjoint `effects(invalidates(c.tag))` MUST be rejected.
    check_rejected_with_reason(
        &dir,
        "managed_context_violates_empty_invalidates",
        r#"
enum ChanState { Open, Closed }
resource Channel { state: ChanState }
package Context { manages handle: Channel; pub tag: i32; }
fn change(c: &Context) {
    let d: Channel = c.handle
    d.state = ChanState.Closed
}
fn bad(c: &Context) contract { effects(invalidates()); } {
    change(c)
}
fn main() -> i32 {
    let ch = Channel { state: ChanState.Open }
    let ctx = Context { handle: give ch, tag: 7 }
    bad(&ctx)
    return 0
}
"#,
        "E-SAFETY-CHECK",
        "invalidates",
    );

    check_rejected_with_reason(
        &dir,
        "managed_context_violates_disjoint_tag_invalidates",
        r#"
enum ChanState { Open, Closed }
resource Channel { state: ChanState }
package Context { manages handle: Channel; pub tag: i32; }
fn change(c: &Context) {
    let d: Channel = c.handle
    d.state = ChanState.Closed
}
fn bad(c: &Context) contract { effects(invalidates(c.tag)); } {
    change(c)
}
fn main() -> i32 {
    let ch = Channel { state: ChanState.Open }
    let ctx = Context { handle: give ch, tag: 7 }
    bad(&ctx)
    return 0
}
"#,
        "E-SAFETY-CHECK",
        "invalidates",
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn raw_wire_managed_describe_protocol_dyn_and_projection_checked() {
    use zil_ir::body::{
        AdtDef, AdtKind, Block, BlockId, Body, Callee, ConstValue, ContractFieldPath, DynDef,
        DynImpl, DynSlotSig, FieldDef, FieldIdx, FnId, FnRole, FnSig, Function, GenericArgs, IntTy,
        Local, LocalDecl, Module, Mutability, Operand, ParamEnv, Place, ProtocolStateSet, Region,
        ResumeMode, StateContractClause, Terminator, TyKind, TyTable, VariantDef, decode_module,
        encode_module,
    };

    let make_channel_types = |tys: &mut TyTable| {
        let empty_args = tys.intern_args(GenericArgs {
            tys: vec![],
            regions: vec![],
        });
        let st_adt = tys.add_adt(AdtDef {
            name: "ChanState".into(),
            kind: AdtKind::Enum,
            type_params: 0,
            region_params: 0,
            variants: vec![
                VariantDef {
                    name: "Open".into(),
                    fields: vec![],
                },
                VariantDef {
                    name: "Closed".into(),
                    fields: vec![],
                },
            ],
            user_drop: false,
        });
        let st_ty = tys.intern(TyKind::Adt {
            adt: st_adt,
            args: empty_args,
        });
        let ch_adt = tys.add_adt(AdtDef {
            name: "Channel".into(),
            kind: AdtKind::Resource,
            type_params: 0,
            region_params: 0,
            variants: vec![VariantDef {
                name: "Channel".into(),
                fields: vec![FieldDef {
                    name: "state".into(),
                    ty: st_ty,
                }],
            }],
            user_drop: false,
        });
        let ch_ty = tys.intern(TyKind::Adt {
            adt: ch_adt,
            args: empty_args,
        });
        let managed_ch = tys.intern(TyKind::Managed(ch_ty));
        let ref_managed_ch = tys.intern(TyKind::Ref {
            region: Region::Static,
            mutability: Mutability::Shared,
            pointee: managed_ch,
        });
        (empty_args, managed_ch, ref_managed_ch)
    };

    // --- 1. Raw/Wire: `&Managed<Channel>` uncontracted Direct call (state preservation vs H rollback) ---
    {
        #[derive(Clone, Copy)]
        enum ManagedCallContract {
            PreserveOpen,
            WideNoRollback,
            WideWithRollback,
        }
        let build_managed_call_module = |mode: ManagedCallContract| -> Module {
            let mut tys = TyTable::new();
            let unit_ty = tys.intern(TyKind::Unit);
            let (_, _, ref_managed_ch) = make_channel_types(&mut tys);
            let open = ProtocolStateSet::single(0);
            let both = open.union(ProtocolStateSet::single(1));
            let contracts = match mode {
                ManagedCallContract::PreserveOpen => vec![
                    StateContractClause::RequiresState {
                        param: 0,
                        states: open,
                    },
                    StateContractClause::ProvidesState {
                        param: 0,
                        states: open,
                    },
                ],
                ManagedCallContract::WideNoRollback => vec![
                    StateContractClause::RequiresState {
                        param: 0,
                        states: open,
                    },
                    StateContractClause::ProvidesState {
                        param: 0,
                        states: both,
                    },
                ],
                ManagedCallContract::WideWithRollback => vec![
                    StateContractClause::RequiresState {
                        param: 0,
                        states: open,
                    },
                    StateContractClause::ProvidesState {
                        param: 0,
                        states: both,
                    },
                    StateContractClause::AllowsRollback { param: 0 },
                ],
            };
            let caller = Function {
                name: "managed_caller".into(),
                sig: FnSig {
                    params: vec![ref_managed_ch],
                    result: unit_ty,
                    type_params: 0,
                    region_params: 0,
                    role: FnRole::Normal,
                    resume_mode: ResumeMode::Local,
                    state_contracts: contracts,
                },
                body: Body {
                    param_count: 1,
                    locals: vec![
                        LocalDecl {
                            ty: ref_managed_ch,
                            name: Some("ch".into()),
                        },
                        LocalDecl {
                            ty: unit_ty,
                            name: Some("res".into()),
                        },
                    ],
                    captures: vec![],
                    closure: None,
                    result: unit_ty,
                    param_env: ParamEnv::default(),
                    drop_flags: vec![],
                    blocks: vec![
                        Block {
                            instructions: vec![],
                            terminator: Terminator::Call {
                                callee: Callee::Direct(FnId(1)),
                                args: vec![Operand::Copy(Place::local(Local(0)))],
                                dest: Place::local(Local(1)),
                                target: BlockId(1),
                                cancel: None,
                            },
                            terminator_span: zil_base::Span::default(),
                        },
                        Block {
                            instructions: vec![],
                            terminator: Terminator::Return {
                                value: Operand::Const(ConstValue::Unit),
                            },
                            terminator_span: zil_base::Span::default(),
                        },
                    ],
                },
            };
            let callee = Function {
                name: "opaque_change".into(),
                sig: FnSig {
                    params: vec![ref_managed_ch],
                    result: unit_ty,
                    type_params: 0,
                    region_params: 0,
                    role: FnRole::Normal,
                    resume_mode: ResumeMode::Local,
                    state_contracts: vec![],
                },
                body: Body {
                    param_count: 1,
                    locals: vec![LocalDecl {
                        ty: ref_managed_ch,
                        name: Some("ch".into()),
                    }],
                    captures: vec![],
                    closure: None,
                    result: unit_ty,
                    param_env: ParamEnv::default(),
                    drop_flags: vec![],
                    blocks: vec![Block {
                        instructions: vec![],
                        terminator: Terminator::Return {
                            value: Operand::Const(ConstValue::Unit),
                        },
                        terminator_span: zil_base::Span::default(),
                    }],
                },
            };
            Module {
                tys,
                functions: vec![caller, callee],
                foreigns: vec![],
                statics: vec![],
                dyn_impls: vec![],
            }
        };

        // 1a. Negative: preserving Open across uncontracted `&Managed<Channel>` call rejected.
        let m1 = build_managed_call_module(ManagedCallContract::PreserveOpen);
        let decoded = decode_module(&encode_module(&m1)).expect("decode managed preserve module");
        let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
            .expect("solve managed preserve module");
        let err =
            zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 1_000_000 })
                .expect_err("checker must reject preserving Open across &Managed<Channel> call");
        assert_eq!(err.function, FnId(0));
        assert!(matches!(
            err.kind,
            zil_check::CheckErrorKind::CertificateShape(_)
        ));

        // 1b. Negative: widening to Open|Closed without AllowsRollback rejected by rollback H check.
        let m2 = build_managed_call_module(ManagedCallContract::WideNoRollback);
        let decoded = decode_module(&encode_module(&m2)).expect("decode managed rollback module");
        let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
            .expect("solve managed rollback module");
        let err =
            zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 1_000_000 })
                .expect_err("checker must reject &Managed<Channel> call lacking AllowsRollback");
        assert_eq!(err.function, FnId(0));
        assert!(matches!(
            err.kind,
            zil_check::CheckErrorKind::CertificateShape(_)
        ));

        // 1c. Positive: Open|Closed WITH explicit AllowsRollback(0) accepted.
        let m3 = build_managed_call_module(ManagedCallContract::WideWithRollback);
        let decoded = decode_module(&encode_module(&m3)).expect("decode managed ok module");
        let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
            .expect("solve managed ok module");
        zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 1_000_000 })
            .expect(
                "checker must accept &Managed<Channel> call with ProvidesState + AllowsRollback",
            );
    }

    // --- 2. Raw/Wire: `Provider { channel: Managed<Channel> }` behind `&self` `DynSlot` ---
    {
        let mut tys = TyTable::new();
        let unit_ty = tys.intern(TyKind::Unit);
        let (empty_args, managed_ch, _) = make_channel_types(&mut tys);
        let prov_adt = tys.add_adt(AdtDef {
            name: "Provider".into(),
            kind: AdtKind::Package,
            type_params: 0,
            region_params: 0,
            variants: vec![VariantDef {
                name: "Provider".into(),
                fields: vec![FieldDef {
                    name: "channel".into(),
                    ty: managed_ch,
                }],
            }],
            user_drop: false,
        });
        let prov_ty = tys.intern(TyKind::Adt {
            adt: prov_adt,
            args: empty_args,
        });
        let ref_prov = tys.intern(TyKind::Ref {
            region: Region::Static,
            mutability: Mutability::Shared,
            pointee: prov_ty,
        });
        let dyn_id = tys.add_dyn(DynDef {
            name: "Inspect".into(),
            slots: vec![DynSlotSig {
                name: "inspect".into(),
                receiver: Some(Mutability::Shared),
                params: vec![],
                result: unit_ty,
            }],
        });
        let dyn_ty = tys.intern(TyKind::Dyn(dyn_id));
        let ref_dyn = tys.intern(TyKind::Ref {
            region: Region::Static,
            mutability: Mutability::Shared,
            pointee: dyn_ty,
        });
        let caller = Function {
            name: "managed_dyn_caller".into(),
            sig: FnSig {
                params: vec![ref_dyn],
                result: unit_ty,
                type_params: 0,
                region_params: 0,
                role: FnRole::Normal,
                resume_mode: ResumeMode::Local,
                state_contracts: vec![StateContractClause::InvalidationUpperBound],
            },
            body: Body {
                param_count: 1,
                locals: vec![
                    LocalDecl {
                        ty: ref_dyn,
                        name: Some("r".into()),
                    },
                    LocalDecl {
                        ty: unit_ty,
                        name: Some("res".into()),
                    },
                ],
                captures: vec![],
                closure: None,
                result: unit_ty,
                param_env: ParamEnv::default(),
                drop_flags: vec![],
                blocks: vec![
                    Block {
                        instructions: vec![],
                        terminator: Terminator::Call {
                            callee: Callee::DynSlot {
                                object: Operand::Copy(Place::local(Local(0))),
                                slot: 0,
                            },
                            args: vec![],
                            dest: Place::local(Local(1)),
                            target: BlockId(1),
                            cancel: None,
                        },
                        terminator_span: zil_base::Span::default(),
                    },
                    Block {
                        instructions: vec![],
                        terminator: Terminator::Return {
                            value: Operand::Const(ConstValue::Unit),
                        },
                        terminator_span: zil_base::Span::default(),
                    },
                ],
            },
        };
        let impl_fn = Function {
            name: "inspect_provider".into(),
            sig: FnSig {
                params: vec![ref_prov],
                result: unit_ty,
                type_params: 0,
                region_params: 0,
                role: FnRole::DynSlotImpl {
                    dyn_id,
                    concrete_ty: prov_ty,
                    slot: 0,
                },
                resume_mode: ResumeMode::Local,
                state_contracts: vec![],
            },
            body: Body {
                param_count: 1,
                locals: vec![LocalDecl {
                    ty: ref_prov,
                    name: Some("self".into()),
                }],
                captures: vec![],
                closure: None,
                result: unit_ty,
                param_env: ParamEnv::default(),
                drop_flags: vec![],
                blocks: vec![Block {
                    instructions: vec![],
                    terminator: Terminator::Return {
                        value: Operand::Const(ConstValue::Unit),
                    },
                    terminator_span: zil_base::Span::default(),
                }],
            },
        };
        let m = Module {
            tys,
            functions: vec![caller, impl_fn],
            foreigns: vec![],
            statics: vec![],
            dyn_impls: vec![DynImpl {
                dyn_id,
                concrete_ty: prov_ty,
                methods: vec![FnId(1)],
            }],
        };
        let decoded = decode_module(&encode_module(&m)).expect("decode managed dyn module");
        let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
            .expect("solve managed dyn module");
        let err =
            zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 1_000_000 })
                .expect_err(
                    "checker must reject &dyn (&self) when provider holds Managed<Channel>",
                );
        assert_eq!(err.function, FnId(0));
        assert!(matches!(
            err.kind,
            zil_check::CheckErrorKind::CertificateShape(_)
        ));
    }

    // --- 3. Raw/Wire: `&Context { handle: Managed<Channel>, tag: i32 }` precise field projection ---
    {
        let build_ctx_module = |declared_field: Option<u32>| -> Module {
            let mut tys = TyTable::new();
            let unit_ty = tys.intern(TyKind::Unit);
            let i32_ty = tys.intern(TyKind::Int(IntTy::I32));
            let (empty_args, managed_ch, _) = make_channel_types(&mut tys);
            let ctx_adt = tys.add_adt(AdtDef {
                name: "Context".into(),
                kind: AdtKind::Package,
                type_params: 0,
                region_params: 0,
                variants: vec![VariantDef {
                    name: "Context".into(),
                    fields: vec![
                        FieldDef {
                            name: "handle".into(),
                            ty: managed_ch,
                        },
                        FieldDef {
                            name: "tag".into(),
                            ty: i32_ty,
                        },
                    ],
                }],
                user_drop: false,
            });
            let ctx_ty = tys.intern(TyKind::Adt {
                adt: ctx_adt,
                args: empty_args,
            });
            let ref_ctx = tys.intern(TyKind::Ref {
                region: Region::Static,
                mutability: Mutability::Shared,
                pointee: ctx_ty,
            });
            let mut contracts = vec![StateContractClause::InvalidationUpperBound];
            if let Some(fi) = declared_field {
                contracts.push(StateContractClause::Invalidates {
                    param: 0,
                    path: ContractFieldPath::from_slice(&[FieldIdx(fi)]).unwrap(),
                });
            }
            let caller = Function {
                name: "managed_ctx_caller".into(),
                sig: FnSig {
                    params: vec![ref_ctx],
                    result: unit_ty,
                    type_params: 0,
                    region_params: 0,
                    role: FnRole::Normal,
                    resume_mode: ResumeMode::Local,
                    state_contracts: contracts,
                },
                body: Body {
                    param_count: 1,
                    locals: vec![
                        LocalDecl {
                            ty: ref_ctx,
                            name: Some("c".into()),
                        },
                        LocalDecl {
                            ty: unit_ty,
                            name: Some("res".into()),
                        },
                    ],
                    captures: vec![],
                    closure: None,
                    result: unit_ty,
                    param_env: ParamEnv::default(),
                    drop_flags: vec![],
                    blocks: vec![
                        Block {
                            instructions: vec![],
                            terminator: Terminator::Call {
                                callee: Callee::Direct(FnId(1)),
                                args: vec![Operand::Copy(Place::local(Local(0)))],
                                dest: Place::local(Local(1)),
                                target: BlockId(1),
                                cancel: None,
                            },
                            terminator_span: zil_base::Span::default(),
                        },
                        Block {
                            instructions: vec![],
                            terminator: Terminator::Return {
                                value: Operand::Const(ConstValue::Unit),
                            },
                            terminator_span: zil_base::Span::default(),
                        },
                    ],
                },
            };
            let callee = Function {
                name: "opaque_ctx_callee".into(),
                sig: FnSig {
                    params: vec![ref_ctx],
                    result: unit_ty,
                    type_params: 0,
                    region_params: 0,
                    role: FnRole::Normal,
                    resume_mode: ResumeMode::Local,
                    state_contracts: vec![],
                },
                body: Body {
                    param_count: 1,
                    locals: vec![LocalDecl {
                        ty: ref_ctx,
                        name: Some("c".into()),
                    }],
                    captures: vec![],
                    closure: None,
                    result: unit_ty,
                    param_env: ParamEnv::default(),
                    drop_flags: vec![],
                    blocks: vec![Block {
                        instructions: vec![],
                        terminator: Terminator::Return {
                            value: Operand::Const(ConstValue::Unit),
                        },
                        terminator_span: zil_base::Span::default(),
                    }],
                },
            };
            Module {
                tys,
                functions: vec![caller, callee],
                foreigns: vec![],
                statics: vec![],
                dyn_impls: vec![],
            }
        };

        // 3a. Negative: empty upper bound rejected.
        let m_empty = build_ctx_module(None);
        let decoded = decode_module(&encode_module(&m_empty)).expect("decode managed ctx empty");
        let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
            .expect("solve managed ctx empty");
        let err =
            zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 1_000_000 })
                .expect_err("checker must reject &Context<Managed> under empty bound");
        assert_eq!(err.function, FnId(0));
        assert!(matches!(
            err.kind,
            zil_check::CheckErrorKind::CertificateShape(_)
        ));

        // 3b. Negative: disjoint `c.tag` (field 1) rejected.
        let m_tag = build_ctx_module(Some(1));
        let decoded = decode_module(&encode_module(&m_tag)).expect("decode managed ctx tag");
        let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
            .expect("solve managed ctx tag");
        let err =
            zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 1_000_000 })
                .expect_err("checker must reject disjoint c.tag for &Context<Managed>");
        assert_eq!(err.function, FnId(0));
        assert!(matches!(
            err.kind,
            zil_check::CheckErrorKind::CertificateShape(_)
        ));

        // 3c. Positive: exact `c.handle` (field 0) accepted without widening to `c.tag`.
        let m_handle = build_ctx_module(Some(0));
        let decoded = decode_module(&encode_module(&m_handle)).expect("decode managed ctx handle");
        let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
            .expect("solve managed ctx handle");
        zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 1_000_000 })
            .expect("checker must accept exact c.handle upper bound");
    }
}

#[test]
fn nested_resource_alias_and_apart_controls_execute_natively_on_dual_backends() {
    let dir = temp_dir("nested-resource-alias-native");
    let src = r#"
enum ChanState { Open, Closed }
resource Channel { state: ChanState }
resource OtherRes { state: ChanState }
package Context { manages handle: Channel; pub tag: i32; }
package OtherContext { manages other: OtherRes; pub tag: i32; }
struct ContextBorrow<life a> { pub handle: &'a Managed<Channel>; pub tag: i32; }

fn mutate_context(c: &Context) {
    let d: Channel = c.handle
    d.state = ChanState.Closed
}

fn mutate_context_contracted(c: &Context)
    contract { effects(invalidates(c.handle)); }
{
    let d: Channel = c.handle
    d.state = ChanState.Closed
}

fn read_tag_only(c: &Context) -> i32
    contract { effects(invalidates(c.tag)); }
{
    return c.tag
}

fn mutate_other(o: &OtherContext) {
    let d: OtherRes = o.other
    d.state = ChanState.Closed
}

// 1. Explicit `requires(apart(ctx, ch))`: unannotated callee + contracted callee + inline Describe
//    all preserve `ch == Open` while closing `ctx.handle`.
fn apart_mutate_unannotated(ctx: &Context, ch: &Managed<Channel>)
    contract {
        requires(state(ch, ChanState.Open), apart(ctx, ch));
        provides(state(ch, ChanState.Open));
    }
{
    mutate_context(ctx)
}

fn apart_mutate_contracted(ctx: &Context, ch: &Managed<Channel>)
    contract {
        requires(state(ch, ChanState.Open), apart(ctx, ch));
        provides(state(ch, ChanState.Open));
    }
{
    mutate_context_contracted(ctx)
}

fn apart_mutate_inline(ctx: &Context, ch: &Managed<Channel>)
    contract {
        requires(state(ch, ChanState.Open), apart(ctx, ch));
        provides(state(ch, ChanState.Open));
    }
{
    let d: Channel = ctx.handle
    d.state = ChanState.Closed
}

// 2. Disjoint field upper bound `effects(invalidates(c.tag))` does NOT require `apart(ctx, ch)`.
fn disjoint_tag_preserves_ch(ctx: &Context, ch: &Managed<Channel>) -> i32
    contract {
        requires(state(ch, ChanState.Open));
        provides(state(ch, ChanState.Open));
    }
{
    return read_tag_only(ctx)
}

// 3. Unrelated resource type (`OtherRes` vs `Channel`) does NOT require `apart`.
fn distinct_resource_type_preserves_ch(o: &OtherContext, ch: &Managed<Channel>)
    contract {
        requires(state(ch, ChanState.Open));
        provides(state(ch, ChanState.Open));
    }
{
    mutate_other(o)
}

// 4. By-value uniquely owned `Context` cannot alias `ch: &Managed<Channel>`.
fn owned_context_preserves_ch(ctx: Context, ch: &Managed<Channel>)
    contract {
        requires(state(ch, ChanState.Open));
        provides(state(ch, ChanState.Open));
    }
{
    mutate_context(&ctx)
}

// 5. Borrowed aggregate `ContextBorrow<'a>` with explicit `requires(apart(cb, ch))`.
fn apart_borrow_mutate<life a>(cb: ContextBorrow<'a>, ch: &Managed<Channel>)
    contract {
        requires(state(ch, ChanState.Open), apart(cb, ch));
        provides(state(ch, ChanState.Open));
    }
{
    let d: Channel = cb.handle
    d.state = ChanState.Closed
}

fn main() -> i32 {
    let ch_main = Channel { state: ChanState.Open }
    let ctx1 = Context { handle: Channel { state: ChanState.Open }, tag: 11 }
    let ctx2 = Context { handle: Channel { state: ChanState.Open }, tag: 22 }
    let ctx3 = Context { handle: Channel { state: ChanState.Open }, tag: 33 }
    let other_ctx = OtherContext { other: OtherRes { state: ChanState.Open }, tag: 44 }

    let tag_val = disjoint_tag_preserves_ch(&ctx1, &ch_main)
    apart_mutate_unannotated(&ctx1, &ch_main)
    apart_mutate_contracted(&ctx2, &ch_main)
    apart_mutate_inline(&ctx3, &ch_main)
    distinct_resource_type_preserves_ch(&other_ctx, &ch_main)
    owned_context_preserves_ch(Context { handle: Channel { state: ChanState.Open }, tag: 55 }, &ch_main)

    let ch_other = Channel { state: ChanState.Open }
    let cb = ContextBorrow { handle: &ch_other, tag: 66 }
    apart_borrow_mutate(cb, &ch_main)

    match ch_main.state {
        ChanState.Open() => {
            match ctx1.handle.state {
                ChanState.Closed() => {
                    match ch_other.state {
                        ChanState.Closed() => {
                            if tag_val == 11 {
                                return 0
                            }
                        }
                        ChanState.Open() => {}
                    }
                }
                ChanState.Open() => {}
            }
        }
        ChanState.Closed() => {}
    }
    return 1
}
"#;
    let entry = write_source(&dir, "main.zil", src);
    let (code, stdout) = build_and_run_dual_with_c(&dir, &entry, &[]);
    assert_eq!(code, 0);
    assert!(stdout.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn nested_resource_alias_and_apart_violations_are_rejected() {
    let dir = temp_dir("nested-resource-alias-reject");

    // 1. Exact P0-9 PoC: `&Context` holding `manages handle: Channel` + `ch: &Managed<Channel>`
    //    without `apart(ctx, ch)`: calling unannotated `mutate_context(ctx)` violates `provides(state(ch, Open))`.
    check_rejected_with_reason(
        &dir,
        "nested_managed_unannotated_callee_violates_state_preservation",
        r#"
enum ChanState { Open, Closed }
resource Channel { state: ChanState }
package Context { manages handle: Channel; pub tag: i32; }
fn mutate_context(c: &Context) {
    let d: Channel = c.handle
    d.state = ChanState.Closed
}
fn bad(ctx: &Context, ch: &Managed<Channel>)
    contract {
        requires(state(ch, ChanState.Open));
        provides(state(ch, ChanState.Open));
    }
{
    mutate_context(ctx)
}
fn main() -> i32 {
    let ctx = Context { handle: Channel { state: ChanState.Open }, tag: 1 }
    bad(&ctx, &ctx.handle)
    return 0
}
"#,
        "E-SAFETY-CHECK",
        "provides(state(...))",
    );

    // 2. Hidden `H` rollback (`Open -> Closed -> Open`) through unannotated `mutate_context(ctx)`
    //    must be rejected even when `provides(state(ch, Open, Closed))` allows both states.
    check_rejected_with_reason(
        &dir,
        "nested_managed_unannotated_callee_triggers_hidden_rollback_h",
        r#"
enum ChanState { Open, Closed }
resource Channel { state: ChanState }
package Context { manages handle: Channel; pub tag: i32; }
fn bounce_context(c: &Context) {
    let d: Channel = c.handle
    d.state = ChanState.Closed
    d.state = ChanState.Open
}
fn bad(ctx: &Context, ch: &Managed<Channel>)
    contract {
        requires(state(ch, ChanState.Open));
        provides(state(ch, ChanState.Open, ChanState.Closed));
    }
{
    bounce_context(ctx)
}
fn main() -> i32 {
    let ctx = Context { handle: Channel { state: ChanState.Open }, tag: 1 }
    bad(&ctx, &ctx.handle)
    return 0
}
"#,
        "E-SAFETY-CHECK",
        "rollback",
    );

    // 3. Inline `Rvalue::Describe` (`let d: Channel = ctx.handle; d.state = ChanState.Closed`)
    //    must propagate `ch`'s protocol identity into `d` and reject `provides(state(ch, Open))`.
    check_rejected_with_reason(
        &dir,
        "nested_managed_inline_describe_violates_state_preservation",
        r#"
enum ChanState { Open, Closed }
resource Channel { state: ChanState }
package Context { manages handle: Channel; pub tag: i32; }
fn bad(ctx: &Context, ch: &Managed<Channel>)
    contract {
        requires(state(ch, ChanState.Open));
        provides(state(ch, ChanState.Open));
    }
{
    let d: Channel = ctx.handle
    d.state = ChanState.Closed
}
fn main() -> i32 {
    let ctx = Context { handle: Channel { state: ChanState.Open }, tag: 1 }
    bad(&ctx, &ctx.handle)
    return 0
}
"#,
        "E-SAFETY-CHECK",
        "provides(state(...))",
    );

    // 4. Inline `Rvalue::Describe` hidden rollback (`Open -> Closed -> Open`) must be rejected by `H`.
    check_rejected_with_reason(
        &dir,
        "nested_managed_inline_describe_triggers_hidden_rollback_h",
        r#"
enum ChanState { Open, Closed }
resource Channel { state: ChanState }
package Context { manages handle: Channel; pub tag: i32; }
fn bad(ctx: &Context, ch: &Managed<Channel>)
    contract {
        requires(state(ch, ChanState.Open));
    }
{
    let d: Channel = ctx.handle
    d.state = ChanState.Closed
    d.state = ChanState.Open
}
fn main() -> i32 {
    let ctx = Context { handle: Channel { state: ChanState.Open }, tag: 1 }
    bad(&ctx, &ctx.handle)
    return 0
}
"#,
        "E-SAFETY-CHECK",
        "rollback",
    );

    // 5. Contracted callee `effects(invalidates(c.handle))` called on `ctx` without `apart(ctx, ch)`
    //    must invalidate `ch` (both state preservation and hidden rollback).
    check_rejected_with_reason(
        &dir,
        "nested_managed_contracted_invalidates_handle_violates_ch_state",
        r#"
enum ChanState { Open, Closed }
resource Channel { state: ChanState }
package Context { manages handle: Channel; pub tag: i32; }
fn mutate_ctx(c: &Context)
    contract { effects(invalidates(c.handle)); }
{
    let d: Channel = c.handle
    d.state = ChanState.Closed
}
fn bad(ctx: &Context, ch: &Managed<Channel>)
    contract {
        requires(state(ch, ChanState.Open));
        provides(state(ch, ChanState.Open));
    }
{
    mutate_ctx(ctx)
}
fn main() -> i32 {
    let ctx = Context { handle: Channel { state: ChanState.Open }, tag: 1 }
    bad(&ctx, &ctx.handle)
    return 0
}
"#,
        "E-SAFETY-CHECK",
        "provides(state(...))",
    );

    check_rejected_with_reason(
        &dir,
        "nested_managed_contracted_invalidates_handle_triggers_rollback_h",
        r#"
enum ChanState { Open, Closed }
resource Channel { state: ChanState }
package Context { manages handle: Channel; pub tag: i32; }
fn bounce_ctx(c: &Context)
    contract { effects(invalidates(c.handle)); }
{
    let d: Channel = c.handle
    d.state = ChanState.Closed
    d.state = ChanState.Open
}
fn bad(ctx: &Context, ch: &Managed<Channel>)
    contract {
        requires(state(ch, ChanState.Open));
        provides(state(ch, ChanState.Open, ChanState.Closed));
    }
{
    bounce_ctx(ctx)
}
fn main() -> i32 {
    let ctx = Context { handle: Channel { state: ChanState.Open }, tag: 1 }
    bad(&ctx, &ctx.handle)
    return 0
}
"#,
        "E-SAFETY-CHECK",
        "rollback",
    );

    // 6. By-value borrowed aggregate `ContextBorrow<'a> { pub handle: &'a Managed<Channel> }`
    //    without `apart(cb, ch)` must reject both inline Describe and callee mutation.
    check_rejected_with_reason(
        &dir,
        "nested_managed_borrowed_aggregate_inline_violates_ch_state",
        r#"
enum ChanState { Open, Closed }
resource Channel { state: ChanState }
struct ContextBorrow<life a> { pub handle: &'a Managed<Channel>; pub tag: i32; }
fn bad<life a>(cb: ContextBorrow<'a>, ch: &Managed<Channel>)
    contract {
        requires(state(ch, ChanState.Open));
        provides(state(ch, ChanState.Open));
    }
{
    let d: Channel = cb.handle
    d.state = ChanState.Closed
}
fn main() -> i32 {
    let ch = Channel { state: ChanState.Open }
    let cb = ContextBorrow { handle: &ch, tag: 1 }
    bad(cb, &ch)
    return 0
}
"#,
        "E-SAFETY-CHECK",
        "provides(state(...))",
    );

    // 7. 2-level nested package `Outer { pub inner: Context }` inline Describe `o.inner.handle`.
    check_rejected_with_reason(
        &dir,
        "nested_managed_two_level_projection_violates_ch_state",
        r#"
enum ChanState { Open, Closed }
resource Channel { state: ChanState }
package Context { manages handle: Channel; pub tag: i32; }
package Outer { pub inner: Context; pub id: i32; }
fn bad(o: &Outer, ch: &Managed<Channel>)
    contract {
        requires(state(ch, ChanState.Open));
        provides(state(ch, ChanState.Open));
    }
{
    let d: Channel = o.inner.handle
    d.state = ChanState.Closed
}
fn main() -> i32 {
    let o = Outer { inner: Context { handle: Channel { state: ChanState.Open }, tag: 1 }, id: 2 }
    bad(&o, &o.inner.handle)
    return 0
}
"#,
        "E-SAFETY-CHECK",
        "provides(state(...))",
    );

    // 8. `&OwnedDyn<dyn Inspector>` concrete provider holding `Managed<Channel>` + `ch: &Managed<Channel>`
    //    without `apart`: `&self` DynSlot call must invalidate `ch`.
    check_rejected_with_reason(
        &dir,
        "nested_managed_dyn_provider_alongside_ch_violates_state",
        r#"
enum ChanState { Open, Closed }
resource Channel { state: ChanState }
package Provider { manages channel: Channel; }
trait Inspector {
    fn inspect(self: &Self) -> i32;
}
impl Inspector for Provider {
    fn inspect(self: &Self) -> i32 {
        let d: Channel = self.channel
        d.state = ChanState.Closed
        return 1
    }
}
fn bad(r: &OwnedDyn<dyn Inspector>, ch: &Managed<Channel>)
    contract {
        requires(state(ch, ChanState.Open));
        provides(state(ch, ChanState.Open));
    }
{
    let _ = r.inspect()
}
fn main() -> i32 {
    let ch = Channel { state: ChanState.Open }
    match owned_dyn::<dyn Inspector>(Provider { channel: Channel { state: ChanState.Open } }) {
        .err(_) => { return 1 }
        .ok(owner) => {
            bad(&owner, &ch)
        }
    }
    return 0
}
"#,
        "E-SAFETY-CHECK",
        "provides(state(...))",
    );

    // 9. Call-site `requires(apart(ctx, ch))` soundness:
    //    9a. Passing `(&ctx, &ctx.handle)` must be rejected directly by `apart`.
    check_rejected_with_reason(
        &dir,
        "apart_callsite_rejects_ctx_and_ctx_handle",
        r#"
enum ChanState { Open, Closed }
resource Channel { state: ChanState }
package Context { manages handle: Channel; pub tag: i32; }
fn good_apart(ctx: &Context, ch: &Managed<Channel>)
    contract {
        requires(apart(ctx, ch));
    }
{
    let d: Channel = ctx.handle
    d.state = ChanState.Closed
}
fn main() -> i32 {
    let ctx = Context { handle: Channel { state: ChanState.Open }, tag: 1 }
    good_apart(&ctx, &ctx.handle)
    return 0
}
"#,
        "E-SAFETY-CHECK",
        "apart",
    );

    //    9b. Passing `(&cb, &ch)` where `cb = ContextBorrow { handle: &ch }` must be rejected.
    check_rejected_with_reason(
        &dir,
        "apart_callsite_rejects_borrowed_aggregate_containing_ch",
        r#"
enum ChanState { Open, Closed }
resource Channel { state: ChanState }
struct ContextBorrow<life a> { pub handle: &'a Managed<Channel>; pub tag: i32; }
fn good_apart<life a>(cb: ContextBorrow<'a>, ch: &Managed<Channel>)
    contract {
        requires(state(ch, ChanState.Open), apart(cb, ch));
        provides(state(ch, ChanState.Open));
    }
{
    let d: Channel = cb.handle
    d.state = ChanState.Closed
}
fn main() -> i32 {
    let ch = Channel { state: ChanState.Open }
    let cb = ContextBorrow { handle: &ch, tag: 1 }
    good_apart(cb, &ch)
    return 0
}
"#,
        "E-SAFETY-CHECK",
        "apart",
    );

    //    9c. Forwarding `(ctx, ch)` without caller declaring `requires(apart(ctx, ch))` must be rejected.
    check_rejected_with_reason(
        &dir,
        "apart_callsite_rejects_unproven_forwarded_ctx_and_ch",
        r#"
enum ChanState { Open, Closed }
resource Channel { state: ChanState }
package Context { manages handle: Channel; pub tag: i32; }
fn good_apart(ctx: &Context, ch: &Managed<Channel>)
    contract {
        requires(state(ch, ChanState.Open), apart(ctx, ch));
        provides(state(ch, ChanState.Open));
    }
{
    let d: Channel = ctx.handle
    d.state = ChanState.Closed
}
fn bad_forward(ctx: &Context, ch: &Managed<Channel>)
    contract {
        requires(state(ch, ChanState.Open));
        provides(state(ch, ChanState.Open));
    }
{
    good_apart(ctx, ch)
}
fn main() -> i32 {
    let ctx = Context { handle: Channel { state: ChanState.Open }, tag: 1 }
    let ch = Channel { state: ChanState.Open }
    bad_forward(&ctx, &ch)
    return 0
}
"#,
        "E-SAFETY-CHECK",
        "apart",
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn raw_wire_nested_resource_alias_and_managed_create_checked() {
    use zil_ir::body::{
        AdtDef, AdtKind, AggregateKind, Block, BlockId, Body, Callee, ConstValue, FieldDef,
        FieldIdx, FieldRef, FnId, FnRole, FnSig, Function, GenericArgs, Instruction,
        InstructionKind, IntTy, IntrinsicOp, Local, LocalDecl, Module, Mutability, Operand,
        ParamEnv, Place, PlaceRoot, Projection, ProtocolStateSet, Region, ResumeMode, Rvalue,
        StateContractClause, Terminator, TyKind, TyTable, VariantDef, VariantIdx, decode_module,
        encode_module,
    };

    let make_types = |tys: &mut TyTable| {
        let empty_args = tys.intern_args(GenericArgs {
            tys: vec![],
            regions: vec![],
        });
        let unit_ty = tys.intern(TyKind::Unit);
        let i32_ty = tys.intern(TyKind::Int(IntTy::I32));
        let st_adt = tys.add_adt(AdtDef {
            name: "ChanState".into(),
            kind: AdtKind::Enum,
            type_params: 0,
            region_params: 0,
            variants: vec![
                VariantDef {
                    name: "Open".into(),
                    fields: vec![],
                },
                VariantDef {
                    name: "Closed".into(),
                    fields: vec![],
                },
            ],
            user_drop: false,
        });
        let st_ty = tys.intern(TyKind::Adt {
            adt: st_adt,
            args: empty_args,
        });
        let ch_adt = tys.add_adt(AdtDef {
            name: "Channel".into(),
            kind: AdtKind::Resource,
            type_params: 0,
            region_params: 0,
            variants: vec![VariantDef {
                name: "Channel".into(),
                fields: vec![FieldDef {
                    name: "state".into(),
                    ty: st_ty,
                }],
            }],
            user_drop: false,
        });
        let ch_ty = tys.intern(TyKind::Adt {
            adt: ch_adt,
            args: empty_args,
        });
        let managed_ch = tys.intern(TyKind::Managed(ch_ty));
        let desc_ch = tys.intern(TyKind::Desc(ch_ty));
        let ref_managed_ch = tys.intern(TyKind::Ref {
            region: Region::Static,
            mutability: Mutability::Shared,
            pointee: managed_ch,
        });
        let ctx_adt = tys.add_adt(AdtDef {
            name: "Context".into(),
            kind: AdtKind::Package,
            type_params: 0,
            region_params: 0,
            variants: vec![VariantDef {
                name: "Context".into(),
                fields: vec![
                    FieldDef {
                        name: "handle".into(),
                        ty: managed_ch,
                    },
                    FieldDef {
                        name: "tag".into(),
                        ty: i32_ty,
                    },
                ],
            }],
            user_drop: false,
        });
        let ctx_ty = tys.intern(TyKind::Adt {
            adt: ctx_adt,
            args: empty_args,
        });
        let ref_ctx = tys.intern(TyKind::Ref {
            region: Region::Static,
            mutability: Mutability::Shared,
            pointee: ctx_ty,
        });
        (
            empty_args,
            unit_ty,
            i32_ty,
            st_adt,
            st_ty,
            ch_adt,
            managed_ch,
            desc_ch,
            ref_managed_ch,
            ctx_adt,
            ctx_ty,
            ref_ctx,
        )
    };

    // --- Part 1: Raw/Wire nested `Context { handle: Managed<Channel> }` vs `ch: &Managed<Channel>` ---
    {
        #[derive(Clone, Copy)]
        enum NestedMode {
            PreserveNoApart,
            WideNoRollback,
            PreserveWithApart,
        }
        let build_call_mod = |mode: NestedMode| -> Module {
            let mut tys = TyTable::new();
            let (_, unit_ty, _, _, _, _, _, _, ref_managed_ch, _, _, ref_ctx) =
                make_types(&mut tys);
            let open = ProtocolStateSet::single(0);
            let both = open.union(ProtocolStateSet::single(1));
            let contracts = match mode {
                NestedMode::PreserveNoApart => vec![
                    StateContractClause::RequiresState {
                        param: 1,
                        states: open,
                    },
                    StateContractClause::ProvidesState {
                        param: 1,
                        states: open,
                    },
                ],
                NestedMode::WideNoRollback => vec![
                    StateContractClause::RequiresState {
                        param: 1,
                        states: open,
                    },
                    StateContractClause::ProvidesState {
                        param: 1,
                        states: both,
                    },
                ],
                NestedMode::PreserveWithApart => vec![
                    StateContractClause::RequiresState {
                        param: 1,
                        states: open,
                    },
                    StateContractClause::RequiresApart {
                        param_a: 0,
                        param_b: 1,
                    },
                    StateContractClause::ProvidesState {
                        param: 1,
                        states: open,
                    },
                ],
            };
            let caller = Function {
                name: "nested_ctx_caller".into(),
                sig: FnSig {
                    params: vec![ref_ctx, ref_managed_ch],
                    result: unit_ty,
                    type_params: 0,
                    region_params: 0,
                    role: FnRole::Normal,
                    resume_mode: ResumeMode::Local,
                    state_contracts: contracts,
                },
                body: Body {
                    param_count: 2,
                    locals: vec![
                        LocalDecl {
                            ty: ref_ctx,
                            name: Some("ctx".into()),
                        },
                        LocalDecl {
                            ty: ref_managed_ch,
                            name: Some("ch".into()),
                        },
                        LocalDecl {
                            ty: unit_ty,
                            name: Some("res".into()),
                        },
                    ],
                    captures: vec![],
                    closure: None,
                    result: unit_ty,
                    param_env: ParamEnv::default(),
                    drop_flags: vec![],
                    blocks: vec![
                        Block {
                            instructions: vec![],
                            terminator: Terminator::Call {
                                callee: Callee::Direct(FnId(1)),
                                args: vec![Operand::Copy(Place::local(Local(0)))],
                                dest: Place::local(Local(2)),
                                target: BlockId(1),
                                cancel: None,
                            },
                            terminator_span: zil_base::Span::default(),
                        },
                        Block {
                            instructions: vec![],
                            terminator: Terminator::Return {
                                value: Operand::Const(ConstValue::Unit),
                            },
                            terminator_span: zil_base::Span::default(),
                        },
                    ],
                },
            };
            let callee = Function {
                name: "mutate_context".into(),
                sig: FnSig {
                    params: vec![ref_ctx],
                    result: unit_ty,
                    type_params: 0,
                    region_params: 0,
                    role: FnRole::Normal,
                    resume_mode: ResumeMode::Local,
                    state_contracts: vec![],
                },
                body: Body {
                    param_count: 1,
                    locals: vec![LocalDecl {
                        ty: ref_ctx,
                        name: Some("c".into()),
                    }],
                    captures: vec![],
                    closure: None,
                    result: unit_ty,
                    param_env: ParamEnv::default(),
                    drop_flags: vec![],
                    blocks: vec![Block {
                        instructions: vec![],
                        terminator: Terminator::Return {
                            value: Operand::Const(ConstValue::Unit),
                        },
                        terminator_span: zil_base::Span::default(),
                    }],
                },
            };
            Module {
                tys,
                functions: vec![caller, callee],
                foreigns: vec![],
                statics: vec![],
                dyn_impls: vec![],
            }
        };

        // 1a. Negative: unannotated call on `&Context` without `RequiresApart` violates `ch == Open`.
        let m = build_call_mod(NestedMode::PreserveNoApart);
        let decoded = decode_module(&encode_module(&m)).expect("decode nested call preserve");
        let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
            .expect("solve nested call preserve");
        let err =
            zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 1_000_000 })
                .expect_err("checker must reject preserving ch=Open across &Context call");
        assert_eq!(err.function, FnId(0));
        assert!(matches!(
            err.kind,
            zil_check::CheckErrorKind::CertificateShape(_)
        ));

        // 1b. Negative: widening to Open|Closed without AllowsRollback rejected by rollback H.
        let m = build_call_mod(NestedMode::WideNoRollback);
        let decoded = decode_module(&encode_module(&m)).expect("decode nested call rollback");
        let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
            .expect("solve nested call rollback");
        let err =
            zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 1_000_000 })
                .expect_err("checker must reject hidden rollback H on ch across &Context call");
        assert_eq!(err.function, FnId(0));
        assert!(matches!(
            err.kind,
            zil_check::CheckErrorKind::CertificateShape(_)
        ));

        // 1c. Positive: WITH explicit `RequiresApart(0, 1)`, `ch == Open` is preserved.
        let m = build_call_mod(NestedMode::PreserveWithApart);
        let decoded = decode_module(&encode_module(&m)).expect("decode nested call apart");
        let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
            .expect("solve nested call apart");
        zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 1_000_000 })
            .expect("checker must accept preserving ch=Open when RequiresApart(ctx, ch) holds");

        // 1d & 1e. Inline `Rvalue::Describe((*ctx).handle)` + state write (preserve & rollback H).
        let build_inline_mod = |bounce_back: bool| -> Module {
            let mut tys = TyTable::new();
            let (
                empty_args,
                unit_ty,
                _,
                st_adt,
                st_ty,
                ch_adt,
                _,
                desc_ch,
                ref_managed_ch,
                ctx_adt,
                _,
                ref_ctx,
            ) = make_types(&mut tys);
            let open = ProtocolStateSet::single(0);
            let mut insts = vec![
                Instruction {
                    kind: InstructionKind::Assign {
                        dest: Place::local(Local(2)),
                        value: Rvalue::Describe {
                            place: Place {
                                root: PlaceRoot::Local(Local(0)),
                                projections: vec![
                                    Projection::Deref,
                                    Projection::Field(FieldRef {
                                        adt: ctx_adt,
                                        args: empty_args,
                                        variant: VariantIdx(0),
                                        field: FieldIdx(0),
                                    }),
                                ],
                            },
                        },
                    },
                    span: zil_base::Span::default(),
                },
                Instruction {
                    kind: InstructionKind::Assign {
                        dest: Place::local(Local(3)),
                        value: Rvalue::Aggregate {
                            kind: AggregateKind::Adt {
                                adt: st_adt,
                                variant: VariantIdx(1),
                                args: empty_args,
                            },
                            operands: vec![],
                        },
                    },
                    span: zil_base::Span::default(),
                },
                Instruction {
                    kind: InstructionKind::Assign {
                        dest: Place {
                            root: PlaceRoot::Local(Local(2)),
                            projections: vec![
                                Projection::Deref,
                                Projection::Field(FieldRef {
                                    adt: ch_adt,
                                    args: empty_args,
                                    variant: VariantIdx(0),
                                    field: FieldIdx(0),
                                }),
                            ],
                        },
                        value: Rvalue::Use(Operand::Copy(Place::local(Local(3)))),
                    },
                    span: zil_base::Span::default(),
                },
            ];
            let contracts = if bounce_back {
                insts.push(Instruction {
                    kind: InstructionKind::Assign {
                        dest: Place::local(Local(4)),
                        value: Rvalue::Aggregate {
                            kind: AggregateKind::Adt {
                                adt: st_adt,
                                variant: VariantIdx(0),
                                args: empty_args,
                            },
                            operands: vec![],
                        },
                    },
                    span: zil_base::Span::default(),
                });
                insts.push(Instruction {
                    kind: InstructionKind::Assign {
                        dest: Place {
                            root: PlaceRoot::Local(Local(2)),
                            projections: vec![
                                Projection::Deref,
                                Projection::Field(FieldRef {
                                    adt: ch_adt,
                                    args: empty_args,
                                    variant: VariantIdx(0),
                                    field: FieldIdx(0),
                                }),
                            ],
                        },
                        value: Rvalue::Use(Operand::Copy(Place::local(Local(4)))),
                    },
                    span: zil_base::Span::default(),
                });
                vec![StateContractClause::RequiresState {
                    param: 1,
                    states: open,
                }]
            } else {
                vec![
                    StateContractClause::RequiresState {
                        param: 1,
                        states: open,
                    },
                    StateContractClause::ProvidesState {
                        param: 1,
                        states: open,
                    },
                ]
            };
            let func = Function {
                name: "inline_describe_nested".into(),
                sig: FnSig {
                    params: vec![ref_ctx, ref_managed_ch],
                    result: unit_ty,
                    type_params: 0,
                    region_params: 0,
                    role: FnRole::Normal,
                    resume_mode: ResumeMode::Local,
                    state_contracts: contracts,
                },
                body: Body {
                    param_count: 2,
                    locals: vec![
                        LocalDecl {
                            ty: ref_ctx,
                            name: Some("ctx".into()),
                        },
                        LocalDecl {
                            ty: ref_managed_ch,
                            name: Some("ch".into()),
                        },
                        LocalDecl {
                            ty: desc_ch,
                            name: Some("d".into()),
                        },
                        LocalDecl {
                            ty: st_ty,
                            name: Some("closed".into()),
                        },
                        LocalDecl {
                            ty: st_ty,
                            name: Some("open".into()),
                        },
                    ],
                    captures: vec![],
                    closure: None,
                    result: unit_ty,
                    param_env: ParamEnv::default(),
                    drop_flags: vec![],
                    blocks: vec![Block {
                        instructions: insts,
                        terminator: Terminator::Return {
                            value: Operand::Const(ConstValue::Unit),
                        },
                        terminator_span: zil_base::Span::default(),
                    }],
                },
            };
            Module {
                tys,
                functions: vec![func],
                foreigns: vec![],
                statics: vec![],
                dyn_impls: vec![],
            }
        };

        // 1d. Negative: inline Describe(ctx.handle) + d.state = Closed violates ch=Open.
        let m = build_inline_mod(false);
        let decoded = decode_module(&encode_module(&m)).expect("decode inline describe preserve");
        let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
            .expect("solve inline describe preserve");
        let err =
            zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 1_000_000 })
                .expect_err("checker must reject inline Describe(ctx.handle) state write");
        assert_eq!(err.function, FnId(0));
        assert!(matches!(
            err.kind,
            zil_check::CheckErrorKind::CertificateShape(_)
        ));

        // 1e. Negative: inline Describe(ctx.handle) + Closed -> Open violates rollback H on ch.
        let m = build_inline_mod(true);
        let decoded = decode_module(&encode_module(&m)).expect("decode inline describe rollback");
        let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
            .expect("solve inline describe rollback");
        let err =
            zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 1_000_000 })
                .expect_err("checker must reject inline Describe(ctx.handle) hidden rollback H");
        assert_eq!(err.function, FnId(0));
        assert!(matches!(
            err.kind,
            zil_check::CheckErrorKind::CertificateShape(_)
        ));

        // 1f. Negative: Raw/Wire call site passing `(&ctx, &ctx.handle)` to `RequiresApart(0, 1)`.
        {
            let mut tys = TyTable::new();
            let (
                empty_args,
                unit_ty,
                i32_ty,
                st_adt,
                st_ty,
                _,
                managed_ch,
                _,
                ref_managed_ch,
                ctx_adt,
                ctx_ty,
                ref_ctx,
            ) = make_types(&mut tys);
            let caller = Function {
                name: "bad_apart_callsite".into(),
                sig: FnSig {
                    params: vec![],
                    result: unit_ty,
                    type_params: 0,
                    region_params: 0,
                    role: FnRole::Normal,
                    resume_mode: ResumeMode::Local,
                    state_contracts: vec![],
                },
                body: Body {
                    param_count: 0,
                    locals: vec![
                        LocalDecl {
                            ty: st_ty,
                            name: Some("open_st".into()),
                        },
                        LocalDecl {
                            ty: managed_ch,
                            name: Some("ch".into()),
                        },
                        LocalDecl {
                            ty: ctx_ty,
                            name: Some("ctx".into()),
                        },
                        LocalDecl {
                            ty: ref_ctx,
                            name: Some("r_ctx".into()),
                        },
                        LocalDecl {
                            ty: ref_managed_ch,
                            name: Some("r_handle".into()),
                        },
                        LocalDecl {
                            ty: unit_ty,
                            name: Some("res".into()),
                        },
                    ],
                    captures: vec![],
                    closure: None,
                    result: unit_ty,
                    param_env: ParamEnv::default(),
                    drop_flags: vec![],
                    blocks: vec![
                        Block {
                            instructions: vec![Instruction {
                                kind: InstructionKind::Assign {
                                    dest: Place::local(Local(0)),
                                    value: Rvalue::Aggregate {
                                        kind: AggregateKind::Adt {
                                            adt: st_adt,
                                            variant: VariantIdx(0),
                                            args: empty_args,
                                        },
                                        operands: vec![],
                                    },
                                },
                                span: zil_base::Span::default(),
                            }],
                            terminator: Terminator::Call {
                                callee: Callee::Intrinsic(IntrinsicOp::ManagedCreate),
                                args: vec![Operand::Copy(Place::local(Local(0)))],
                                dest: Place::local(Local(1)),
                                target: BlockId(1),
                                cancel: None,
                            },
                            terminator_span: zil_base::Span::default(),
                        },
                        Block {
                            instructions: vec![
                                Instruction {
                                    kind: InstructionKind::Assign {
                                        dest: Place::local(Local(2)),
                                        value: Rvalue::Aggregate {
                                            kind: AggregateKind::Adt {
                                                adt: ctx_adt,
                                                variant: VariantIdx(0),
                                                args: empty_args,
                                            },
                                            operands: vec![
                                                Operand::Move(Place::local(Local(1))),
                                                Operand::Const(ConstValue::Int {
                                                    ty: IntTy::I32,
                                                    bits: 1,
                                                }),
                                            ],
                                        },
                                    },
                                    span: zil_base::Span::default(),
                                },
                                Instruction {
                                    kind: InstructionKind::Assign {
                                        dest: Place::local(Local(3)),
                                        value: Rvalue::Ref {
                                            region: Region::Static,
                                            mutability: Mutability::Shared,
                                            place: Place::local(Local(2)),
                                        },
                                    },
                                    span: zil_base::Span::default(),
                                },
                                Instruction {
                                    kind: InstructionKind::Assign {
                                        dest: Place::local(Local(4)),
                                        value: Rvalue::Ref {
                                            region: Region::Static,
                                            mutability: Mutability::Shared,
                                            place: Place {
                                                root: PlaceRoot::Local(Local(2)),
                                                projections: vec![Projection::Field(FieldRef {
                                                    adt: ctx_adt,
                                                    args: empty_args,
                                                    variant: VariantIdx(0),
                                                    field: FieldIdx(0),
                                                })],
                                            },
                                        },
                                    },
                                    span: zil_base::Span::default(),
                                },
                            ],
                            terminator: Terminator::Call {
                                callee: Callee::Direct(FnId(1)),
                                args: vec![
                                    Operand::Copy(Place::local(Local(3))),
                                    Operand::Copy(Place::local(Local(4))),
                                ],
                                dest: Place::local(Local(5)),
                                target: BlockId(2),
                                cancel: None,
                            },
                            terminator_span: zil_base::Span::default(),
                        },
                        Block {
                            instructions: vec![Instruction {
                                kind: InstructionKind::Drop {
                                    place: Place::local(Local(2)),
                                },
                                span: zil_base::Span::default(),
                            }],
                            terminator: Terminator::Return {
                                value: Operand::Const(ConstValue::Unit),
                            },
                            terminator_span: zil_base::Span::default(),
                        },
                    ],
                },
            };
            let _ = i32_ty;
            let callee = Function {
                name: "needs_apart".into(),
                sig: FnSig {
                    params: vec![ref_ctx, ref_managed_ch],
                    result: unit_ty,
                    type_params: 0,
                    region_params: 0,
                    role: FnRole::Normal,
                    resume_mode: ResumeMode::Local,
                    state_contracts: vec![StateContractClause::RequiresApart {
                        param_a: 0,
                        param_b: 1,
                    }],
                },
                body: Body {
                    param_count: 2,
                    locals: vec![
                        LocalDecl {
                            ty: ref_ctx,
                            name: Some("ctx".into()),
                        },
                        LocalDecl {
                            ty: ref_managed_ch,
                            name: Some("ch".into()),
                        },
                    ],
                    captures: vec![],
                    closure: None,
                    result: unit_ty,
                    param_env: ParamEnv::default(),
                    drop_flags: vec![],
                    blocks: vec![Block {
                        instructions: vec![],
                        terminator: Terminator::Return {
                            value: Operand::Const(ConstValue::Unit),
                        },
                        terminator_span: zil_base::Span::default(),
                    }],
                },
            };
            let m = Module {
                tys,
                functions: vec![caller, callee],
                foreigns: vec![],
                statics: vec![],
                dyn_impls: vec![],
            };
            let decoded = decode_module(&encode_module(&m)).expect("decode bad apart callsite");
            let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
                .expect("solve bad apart callsite");
            let err = zil_check::check_module(
                decoded,
                &cert,
                zil_check::CheckBudget { steps: 1_000_000 },
            )
            .expect_err("checker must reject (&ctx, &ctx.handle) at RequiresApart callsite");
            assert_eq!(err.function, FnId(0));
            assert!(matches!(
                err.kind,
                zil_check::CheckErrorKind::CertificateShape(_)
            ));
        }
    }

    // --- Part 2: Direct `Callee::Intrinsic(IntrinsicOp::ManagedCreate)` Raw/Wire certificates ---
    {
        let build_managed_create_mod = |init_variant: u32| -> Module {
            let mut tys = TyTable::new();
            let (empty_args, unit_ty, _, st_adt, st_ty, _, managed_ch, _, ref_managed_ch, _, _, _) =
                make_types(&mut tys);
            let open = ProtocolStateSet::single(0);
            let caller = Function {
                name: "managed_create_caller".into(),
                sig: FnSig {
                    params: vec![],
                    result: unit_ty,
                    type_params: 0,
                    region_params: 0,
                    role: FnRole::Normal,
                    resume_mode: ResumeMode::Local,
                    state_contracts: vec![],
                },
                body: Body {
                    param_count: 0,
                    locals: vec![
                        LocalDecl {
                            ty: st_ty,
                            name: Some("st".into()),
                        },
                        LocalDecl {
                            ty: managed_ch,
                            name: Some("ch".into()),
                        },
                        LocalDecl {
                            ty: ref_managed_ch,
                            name: Some("r_ch".into()),
                        },
                        LocalDecl {
                            ty: unit_ty,
                            name: Some("res".into()),
                        },
                    ],
                    captures: vec![],
                    closure: None,
                    result: unit_ty,
                    param_env: ParamEnv::default(),
                    drop_flags: vec![],
                    blocks: vec![
                        Block {
                            instructions: vec![Instruction {
                                kind: InstructionKind::Assign {
                                    dest: Place::local(Local(0)),
                                    value: Rvalue::Aggregate {
                                        kind: AggregateKind::Adt {
                                            adt: st_adt,
                                            variant: VariantIdx(init_variant),
                                            args: empty_args,
                                        },
                                        operands: vec![],
                                    },
                                },
                                span: zil_base::Span::default(),
                            }],
                            terminator: Terminator::Call {
                                callee: Callee::Intrinsic(IntrinsicOp::ManagedCreate),
                                args: vec![Operand::Copy(Place::local(Local(0)))],
                                dest: Place::local(Local(1)),
                                target: BlockId(1),
                                cancel: None,
                            },
                            terminator_span: zil_base::Span::default(),
                        },
                        Block {
                            instructions: vec![Instruction {
                                kind: InstructionKind::Assign {
                                    dest: Place::local(Local(2)),
                                    value: Rvalue::Ref {
                                        region: Region::Static,
                                        mutability: Mutability::Shared,
                                        place: Place::local(Local(1)),
                                    },
                                },
                                span: zil_base::Span::default(),
                            }],
                            terminator: Terminator::Call {
                                callee: Callee::Direct(FnId(1)),
                                args: vec![Operand::Copy(Place::local(Local(2)))],
                                dest: Place::local(Local(3)),
                                target: BlockId(2),
                                cancel: None,
                            },
                            terminator_span: zil_base::Span::default(),
                        },
                        Block {
                            instructions: vec![Instruction {
                                kind: InstructionKind::Drop {
                                    place: Place::local(Local(1)),
                                },
                                span: zil_base::Span::default(),
                            }],
                            terminator: Terminator::Return {
                                value: Operand::Const(ConstValue::Unit),
                            },
                            terminator_span: zil_base::Span::default(),
                        },
                    ],
                },
            };
            let require_open = Function {
                name: "require_open".into(),
                sig: FnSig {
                    params: vec![ref_managed_ch],
                    result: unit_ty,
                    type_params: 0,
                    region_params: 0,
                    role: FnRole::Normal,
                    resume_mode: ResumeMode::Local,
                    state_contracts: vec![StateContractClause::RequiresState {
                        param: 0,
                        states: open,
                    }],
                },
                body: Body {
                    param_count: 1,
                    locals: vec![LocalDecl {
                        ty: ref_managed_ch,
                        name: Some("ch".into()),
                    }],
                    captures: vec![],
                    closure: None,
                    result: unit_ty,
                    param_env: ParamEnv::default(),
                    drop_flags: vec![],
                    blocks: vec![Block {
                        instructions: vec![],
                        terminator: Terminator::Return {
                            value: Operand::Const(ConstValue::Unit),
                        },
                        terminator_span: zil_base::Span::default(),
                    }],
                },
            };
            Module {
                tys,
                functions: vec![caller, require_open],
                foreigns: vec![],
                statics: vec![],
                dyn_impls: vec![],
            }
        };

        // 2a. Positive: `ManagedCreate(ChanState.Open)` satisfies `requires(state(ch, Open))`.
        let m_ok = build_managed_create_mod(0);
        let decoded = decode_module(&encode_module(&m_ok)).expect("decode managed create ok");
        let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
            .expect("solve managed create ok");
        zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 1_000_000 })
            .expect("checker must accept ManagedCreate(Open) passed to requires(state(ch, Open))");

        // 2b. Negative: `ManagedCreate(ChanState.Closed)` rejected by `requires(state(ch, Open))`.
        let m_bad = build_managed_create_mod(1);
        let decoded = decode_module(&encode_module(&m_bad)).expect("decode managed create bad");
        let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
            .expect("solve managed create bad");
        let err =
            zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 1_000_000 })
                .expect_err(
                    "checker must reject ManagedCreate(Closed) passed to requires(state(ch, Open))",
                );
        assert_eq!(err.function, FnId(0));
        assert!(matches!(
            err.kind,
            zil_check::CheckErrorKind::CertificateShape(_)
        ));
    }
}
