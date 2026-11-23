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
