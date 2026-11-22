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
