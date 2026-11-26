//! LLVM 23 opaque-pointer IR emitter directly consuming `zil_check::VerifiedModule`.
//!
//! Only a `VerifiedModule` constructed by `zil_check::check_module` can enter
//! this emitter. Every `InstructionKind`, `Rvalue`, `Terminator`, and `TyKind`
//! is matched exhaustively without wildcard arms.

#![deny(clippy::wildcard_enum_match_arm)]

use std::fmt::Write as _;
use zil_check::VerifiedModule;
use zil_ir::body::{
    AdtKind, AggregateKind, BinOp, Block, Body, Callee, ClosureCallMode, ConstValue, FieldRef,
    FloatTy, FnId, FnRole, Function, InstructionKind, IntTy, IntrinsicOp, Local, Module, OpaqueTy,
    Operand, Place, PlaceRoot, Projection, Rvalue, ScopeExit, Terminator, TrapKind, TyId, TyKind,
    TyTable, UnOp,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BodyEmitError {
    pub message: String,
    pub unsupported: bool,
}

impl BodyEmitError {
    fn new(msg: impl Into<String>) -> Self {
        Self {
            message: msg.into(),
            unsupported: false,
        }
    }
}

/// Emits LLVM 23 opaque-pointer IR for a verified Body IR module as a single unit
/// targeting the host LLVM triple.
///
/// When `emit_c_main` is true and a zero-argument `main` function exists, a C
/// `main()` entry wrapper is emitted that invokes it and returns its `i32`
/// result (or 0 for unit).
pub fn emit_verified_body(
    verified: &VerifiedModule,
    emit_c_main: bool,
) -> Result<String, BodyEmitError> {
    let host = zil_base::TargetProfile::host_llvm();
    emit_verified_body_for_target(verified, emit_c_main, &host.triple)
}

/// Emits LLVM 23 opaque-pointer IR for a verified Body IR module as a single unit
/// for an explicit LLVM target triple (for example `"x86_64-pc-windows-msvc"`).
pub fn emit_verified_body_for_target(
    verified: &VerifiedModule,
    emit_c_main: bool,
    target_triple: &str,
) -> Result<String, BodyEmitError> {
    let mut units = emit_verified_body_units_for_target(verified, emit_c_main, 1, target_triple)?;
    Ok(units.pop().unwrap_or_default())
}

/// Partitions `verified` into `actual_units = requested_units.clamp(1, fn_count.max(1))`
/// independent LLVM 23 IR modules suitable for parallel per-unit object generation.
///
/// Every unit defines its assigned functions (`fn_idx % actual_units == unit_idx`),
/// emits external `declare` prototypes for functions assigned to other units, and
/// emits `private` static byte constants so units link without symbol collisions.
/// The C `main()` wrapper (when requested) is emitted in unit 0.
pub fn emit_verified_body_units(
    verified: &VerifiedModule,
    emit_c_main: bool,
    requested_units: usize,
) -> Result<Vec<String>, BodyEmitError> {
    let host = zil_base::TargetProfile::host_llvm();
    emit_verified_body_units_with_entry_for_target(
        verified,
        emit_c_main,
        requested_units,
        None,
        &host.triple,
    )
}

/// Partitions `verified` into independent LLVM 23 IR modules for an explicit target triple.
pub fn emit_verified_body_units_for_target(
    verified: &VerifiedModule,
    emit_c_main: bool,
    requested_units: usize,
    target_triple: &str,
) -> Result<Vec<String>, BodyEmitError> {
    emit_verified_body_units_with_entry_for_target(
        verified,
        emit_c_main,
        requested_units,
        None,
        target_triple,
    )
}

pub fn emit_verified_body_units_with_entry_for_target(
    verified: &VerifiedModule,
    emit_c_main: bool,
    requested_units: usize,
    entry_name: Option<&str>,
    target_triple: &str,
) -> Result<Vec<String>, BodyEmitError> {
    let module = verified.module();
    let is_windows = target_triple.contains("windows");
    let fn_count = module.functions.len();
    let actual_units = requested_units.clamp(1, fn_count.max(1));

    // Precompute header (target triple, runtime decls, named types, private statics).
    let mut header = String::with_capacity(2048);
    header.push_str("; ZIL Verified Body IR -> LLVM 23 IR\n");
    let _ = writeln!(header, "target triple = \"{target_triple}\"\n");
    header.push_str("declare ptr @calloc(i64, i64) nobuiltin\n");
    header.push_str("declare ptr @realloc(ptr, i64) nobuiltin\n");
    header.push_str("declare void @free(ptr) nobuiltin\n");
    header.push_str("declare void @abort() noreturn\n");
    header.push_str("declare i32 @printf(ptr, ...)\n");
    if is_windows {
        header.push_str("declare void @InitializeSRWLock(ptr)\n");
        header.push_str("declare void @AcquireSRWLockExclusive(ptr)\n");
        header.push_str("declare void @ReleaseSRWLockExclusive(ptr)\n");
        header.push_str("declare ptr @_beginthreadex(ptr, i32, ptr, ptr, i32, ptr)\n");
        header.push_str("declare i32 @WaitForSingleObject(ptr, i32)\n");
        header.push_str("declare i32 @CloseHandle(ptr)\n");
        header.push_str("declare ptr @ConvertThreadToFiber(ptr)\n");
        header.push_str("declare ptr @CreateFiberEx(i64, i64, i32, ptr, ptr)\n");
        header.push_str("declare void @SwitchToFiber(ptr)\n");
        header.push_str("declare void @DeleteFiber(ptr)\n");
        header.push_str("%__zil_task_hdr = type { i32, ptr, ptr, ptr, ptr, ptr, ptr }\n");
    } else {
        header.push_str("declare i32 @getcontext(ptr)\n");
        header.push_str("declare void @makecontext(ptr, ptr, i32, ...)\n");
        header.push_str("declare i32 @swapcontext(ptr, ptr)\n");
        header.push_str("declare i32 @mprotect(ptr, i64, i32)\n");
        header.push_str("declare i32 @pthread_create(ptr, ptr, ptr, ptr)\n");
        header.push_str("declare i32 @pthread_join(i64, ptr)\n");
        header.push_str("declare i32 @pthread_mutex_init(ptr, ptr)\n");
        header.push_str("declare i32 @pthread_mutex_destroy(ptr)\n");
        header.push_str("declare i32 @pthread_mutex_lock(ptr)\n");
        header.push_str("declare i32 @pthread_mutex_unlock(ptr)\n");
        header.push_str(
            "%__zil_task_hdr = type { i32, ptr, [128 x i64], [128 x i64], [9216 x i64], ptr, ptr }\n",
        );
    }
    header.push_str("%__zil_reg_hdr = type { i32, ptr, ptr, ptr }\n");
    header.push_str("%__zil_areg_hdr = type { i32, i32, i32, ptr, ptr, ptr, ptr }\n");

    let target_entry = entry_name.unwrap_or("main");
    let custom_entry = target_entry != "main";
    let mut main_fn: Option<(FnId, String, TyId)> = None;
    let mut syms = Vec::with_capacity(fn_count);
    for (idx, func) in module.functions.iter().enumerate() {
        let fn_id = FnId::from_index(idx);
        let sym = fn_symbol(fn_id, func);
        if (func.name == target_entry || func.name.ends_with(&format!(".{target_entry}")))
            && func.sig.params.is_empty()
            && func.sig.role == FnRole::Normal
        {
            main_fn = Some((fn_id, sym.clone(), func.sig.result));
        }
        syms.push(sym);
    }

    let single_exe = actual_units == 1 && emit_c_main && main_fn.is_some();
    let reach = if single_exe {
        compute_codegen_reachability(module, main_fn.as_ref().map(|(id, _, _)| *id))
    } else {
        CodegenReachability::all(module)
    };
    let runtime_linkage = if single_exe { "internal " } else { "" };

    let mut runtime_defs = String::with_capacity(4096);
    runtime_defs.push_str("@__zil_current_task = thread_local global ptr null\n");
    runtime_defs.push_str("@__zil_task_cancel_trap = thread_local global i1 false\n");
    if is_windows {
        runtime_defs.push_str("@__zil_current_fiber = thread_local global ptr null\n");
    }
    runtime_defs.push_str("@zil_process_argc = global i32 0\n");
    runtime_defs.push_str("@zil_process_argv = global ptr null\n");

    let mut runtime_decls = String::with_capacity(512);
    runtime_decls.push_str("@__zil_current_task = external thread_local global ptr\n");
    runtime_decls.push_str("@__zil_task_cancel_trap = external thread_local global i1\n");
    if is_windows {
        runtime_decls.push_str("@__zil_current_fiber = external thread_local global ptr\n");
        runtime_decls.push_str("declare ptr @__zil_ensure_fiber()\n");
    }
    runtime_decls.push_str("@zil_process_argc = external global i32\n");
    runtime_decls.push_str("@zil_process_argv = external global ptr\n");
    runtime_decls.push_str("declare void @__zil_reg_drop_frame(ptr)\n");
    runtime_decls.push_str("declare void @__zil_task_drop_frame(ptr)\n");
    runtime_decls.push_str("declare void @__zil_areg_mark_closing(ptr)\n");
    runtime_decls.push_str("declare void @__zil_areg_request_close(ptr)\n");
    runtime_decls.push_str("declare i1 @__zil_areg_poll_close(ptr)\n");
    runtime_decls.push_str("declare void @__zil_areg_finish_destroy(ptr)\n");
    runtime_decls.push_str("declare void @__zil_areg_drop_frame(ptr)\n");

    if reach.uses_tasks && is_windows {
        runtime_defs.push_str(
            "define ptr @__zil_ensure_fiber() {\n\
entry:\n\
  %cur = load ptr, ptr @__zil_current_fiber\n\
  %has = icmp ne ptr %cur, null\n\
  br i1 %has, label %ret_cur, label %convert\n\
convert:\n\
  %fib = call ptr @ConvertThreadToFiber(ptr null)\n\
  %ok = icmp ne ptr %fib, null\n\
  br i1 %ok, label %save, label %fail\n\
save:\n\
  store ptr %fib, ptr @__zil_current_fiber\n\
  ret ptr %fib\n\
ret_cur:\n\
  ret ptr %cur\n\
fail:\n\
  call void @abort()\n\
  unreachable\n\
}\n\
define void @__zil_task_drop_frame(ptr %child) {\n\
entry:\n\
  %is_null = icmp eq ptr %child, null\n\
  br i1 %is_null, label %done, label %check\n\
check:\n\
  %st_p = getelementptr inbounds %__zil_task_hdr, ptr %child, i32 0, i32 0\n\
  %st = load i32, ptr %st_p\n\
  switch i32 %st, label %close_unstarted [\n\
    i32 1, label %req_close\n\
    i32 2, label %close_ready\n\
    i32 4, label %drive_close\n\
    i32 5, label %do_free\n\
    i32 6, label %done\n\
  ]\n\
req_close:\n\
  store i32 4, ptr %st_p\n\
  br label %drive_close\n\
drive_close:\n\
  %prev = load ptr, ptr @__zil_current_task\n\
  %cfib = call ptr @__zil_ensure_fiber()\n\
  %cctx = getelementptr inbounds %__zil_task_hdr, ptr %child, i32 0, i32 2\n\
  store ptr %cfib, ptr %cctx\n\
  %wctx = getelementptr inbounds %__zil_task_hdr, ptr %child, i32 0, i32 3\n\
  %wfib = load ptr, ptr %wctx\n\
  store ptr %child, ptr @__zil_current_task\n\
  store ptr %wfib, ptr @__zil_current_fiber\n\
  call void @SwitchToFiber(ptr %wfib)\n\
  store ptr %cfib, ptr @__zil_current_fiber\n\
  store ptr %prev, ptr @__zil_current_task\n\
  %st2 = load i32, ptr %st_p\n\
  %is_closed = icmp eq i32 %st2, 5\n\
  br i1 %is_closed, label %do_free, label %maybe_yield\n\
maybe_yield:\n\
  %has_parent = icmp ne ptr %prev, null\n\
  br i1 %has_parent, label %yield_parent, label %drive_close\n\
yield_parent:\n\
  %pcctx = getelementptr inbounds %__zil_task_hdr, ptr %prev, i32 0, i32 2\n\
  %pcfib = load ptr, ptr %pcctx\n\
  call void @SwitchToFiber(ptr %pcfib)\n\
  br label %drive_close\n\
close_unstarted:\n\
  %da_slot = getelementptr inbounds %__zil_task_hdr, ptr %child, i32 0, i32 6\n\
  %da_fn = load ptr, ptr %da_slot\n\
  %has_da = icmp ne ptr %da_fn, null\n\
  br i1 %has_da, label %call_da, label %close_ready\n\
call_da:\n\
  store ptr null, ptr %da_slot\n\
  call void %da_fn(ptr %child)\n\
  br label %close_ready\n\
close_ready:\n\
  store i32 5, ptr %st_p\n\
  br label %do_free\n\
do_free:\n\
  store i32 6, ptr %st_p\n\
  %wf_slot = getelementptr inbounds %__zil_task_hdr, ptr %child, i32 0, i32 3\n\
  %wf = load ptr, ptr %wf_slot\n\
  %has_wf = icmp ne ptr %wf, null\n\
  br i1 %has_wf, label %del_fiber, label %free_now\n\
del_fiber:\n\
  store ptr null, ptr %wf_slot\n\
  call void @DeleteFiber(ptr %wf)\n\
  br label %free_now\n\
free_now:\n\
  call void @free(ptr %child)\n\
  br label %done\n\
done:\n\
  ret void\n\
}\n",
        );
    } else if reach.uses_tasks {
        runtime_defs.push_str(
            "define void @__zil_task_drop_frame(ptr %child) {\n\
entry:\n\
  %is_null = icmp eq ptr %child, null\n\
  br i1 %is_null, label %done, label %check\n\
check:\n\
  %st_p = getelementptr inbounds %__zil_task_hdr, ptr %child, i32 0, i32 0\n\
  %st = load i32, ptr %st_p\n\
  switch i32 %st, label %close_unstarted [\n\
    i32 1, label %req_close\n\
    i32 2, label %close_ready\n\
    i32 4, label %drive_close\n\
    i32 5, label %do_free\n\
    i32 6, label %done\n\
  ]\n\
req_close:\n\
  store i32 4, ptr %st_p\n\
  br label %drive_close\n\
drive_close:\n\
  %prev = load ptr, ptr @__zil_current_task\n\
  store ptr %child, ptr @__zil_current_task\n\
  %cctx = getelementptr inbounds %__zil_task_hdr, ptr %child, i32 0, i32 2\n\
  %wctx = getelementptr inbounds %__zil_task_hdr, ptr %child, i32 0, i32 3\n\
  %sc1 = call i32 @swapcontext(ptr %cctx, ptr %wctx)\n\
  %sc1_ok = icmp eq i32 %sc1, 0\n\
  br i1 %sc1_ok, label %after_sc1, label %sys_abort\n\
after_sc1:\n\
  store ptr %prev, ptr @__zil_current_task\n\
  %st2 = load i32, ptr %st_p\n\
  %is_closed = icmp eq i32 %st2, 5\n\
  br i1 %is_closed, label %do_free, label %maybe_yield\n\
maybe_yield:\n\
  %has_parent = icmp ne ptr %prev, null\n\
  br i1 %has_parent, label %yield_parent, label %drive_close\n\
yield_parent:\n\
  %pcctx = getelementptr inbounds %__zil_task_hdr, ptr %prev, i32 0, i32 2\n\
  %pwctx = getelementptr inbounds %__zil_task_hdr, ptr %prev, i32 0, i32 3\n\
  %sc2 = call i32 @swapcontext(ptr %pwctx, ptr %pcctx)\n\
  %sc2_ok = icmp eq i32 %sc2, 0\n\
  br i1 %sc2_ok, label %drive_close, label %sys_abort\n\
close_unstarted:\n\
  %da_slot = getelementptr inbounds %__zil_task_hdr, ptr %child, i32 0, i32 6\n\
  %da_fn = load ptr, ptr %da_slot\n\
  %has_da = icmp ne ptr %da_fn, null\n\
  br i1 %has_da, label %call_da, label %close_ready\n\
call_da:\n\
  store ptr null, ptr %da_slot\n\
  call void %da_fn(ptr %child)\n\
  br label %close_ready\n\
close_ready:\n\
  store i32 5, ptr %st_p\n\
  br label %do_free\n\
do_free:\n\
  store i32 6, ptr %st_p\n\
  %gp_slot = getelementptr inbounds %__zil_task_hdr, ptr %child, i32 0, i32 5\n\
  %gp = load ptr, ptr %gp_slot\n\
  %has_gp = icmp ne ptr %gp, null\n\
  br i1 %has_gp, label %unprotect, label %free_now\n\
unprotect:\n\
  %up_rc = call i32 @mprotect(ptr %gp, i64 4096, i32 3)\n\
  %up_ok = icmp eq i32 %up_rc, 0\n\
  br i1 %up_ok, label %free_now, label %sys_abort\n\
free_now:\n\
  call void @free(ptr %child)\n\
  br label %done\n\
sys_abort:\n\
  call void @abort()\n\
  unreachable\n\
done:\n\
  ret void\n\
}\n",
        );
    }
    if reach.uses_regs {
        runtime_defs.push_str(
            "define void @__zil_reg_drop_frame(ptr %frame) {\n\
entry:\n\
  %is_null = icmp eq ptr %frame, null\n\
  br i1 %is_null, label %done, label %close_step\n\
close_step:\n\
  %gate_p = getelementptr inbounds %__zil_reg_hdr, ptr %frame, i32 0, i32 0\n\
  store i32 2, ptr %gate_p\n\
  %cfn_p = getelementptr inbounds %__zil_reg_hdr, ptr %frame, i32 0, i32 2\n\
  %cfn = load ptr, ptr %cfn_p\n\
  %has_cfn = icmp ne ptr %cfn, null\n\
  br i1 %has_cfn, label %check_tok, label %drop_env\n\
check_tok:\n\
  store ptr null, ptr %cfn_p\n\
  %tok_p = getelementptr inbounds %__zil_reg_hdr, ptr %frame, i32 0, i32 1\n\
  %tok = load ptr, ptr %tok_p\n\
  %has_tok = icmp ne ptr %tok, null\n\
  br i1 %has_tok, label %call_close, label %drop_env\n\
call_close:\n\
  call void %cfn(ptr %tok)\n\
  br label %drop_env\n\
drop_env:\n\
  %dfn_p = getelementptr inbounds %__zil_reg_hdr, ptr %frame, i32 0, i32 3\n\
  %dfn = load ptr, ptr %dfn_p\n\
  %has_dfn = icmp ne ptr %dfn, null\n\
  br i1 %has_dfn, label %call_drop, label %do_free\n\
call_drop:\n\
  store ptr null, ptr %dfn_p\n\
  call void %dfn(ptr %frame)\n\
  br label %do_free\n\
do_free:\n\
  call void @free(ptr %frame)\n\
  br label %done\n\
done:\n\
  ret void\n\
}\n",
        );
    }
    if reach.uses_aregs {
        runtime_defs.push_str(
            "define void @__zil_areg_mark_closing(ptr %frame) {\n\
entry:\n\
  %gate_p = getelementptr inbounds %__zil_areg_hdr, ptr %frame, i32 0, i32 0\n\
  %g0 = load atomic i32, ptr %gate_p acquire, align 4\n\
  br label %loop\n\
loop:\n\
  %g = phi i32 [ %g0, %entry ], [ %g_next, %retry ]\n\
  %already = icmp uge i32 %g, 2\n\
  br i1 %already, label %done, label %try_cas\n\
try_cas:\n\
  %target = or i32 %g, 2\n\
  %res = cmpxchg ptr %gate_p, i32 %g, i32 %target acq_rel acquire\n\
  %ok = extractvalue { i32, i1 } %res, 1\n\
  br i1 %ok, label %done, label %retry\n\
retry:\n\
  %g_next = extractvalue { i32, i1 } %res, 0\n\
  br label %loop\n\
done:\n\
  ret void\n\
}\n\
define void @__zil_areg_request_close(ptr %frame) {\n\
entry:\n\
  %is_null = icmp eq ptr %frame, null\n\
  br i1 %is_null, label %done, label %mark\n\
mark:\n\
  call void @__zil_areg_mark_closing(ptr %frame)\n\
  %drv_p = getelementptr inbounds %__zil_areg_hdr, ptr %frame, i32 0, i32 1\n\
  %lock_res = cmpxchg ptr %drv_p, i32 0, i32 1 acq_rel acquire\n\
  %got_lock = extractvalue { i32, i1 } %lock_res, 1\n\
  br i1 %got_lock, label %check_sent, label %done\n\
check_sent:\n\
  %sent_p = getelementptr inbounds %__zil_areg_hdr, ptr %frame, i32 0, i32 2\n\
  %sent = load i32, ptr %sent_p\n\
  %need_send = icmp eq i32 %sent, 0\n\
  br i1 %need_send, label %do_send, label %unlock\n\
do_send:\n\
  store i32 1, ptr %sent_p\n\
  %rfn_p = getelementptr inbounds %__zil_areg_hdr, ptr %frame, i32 0, i32 4\n\
  %rfn = load ptr, ptr %rfn_p\n\
  %tok_p = getelementptr inbounds %__zil_areg_hdr, ptr %frame, i32 0, i32 3\n\
  %tok_i64 = load atomic i64, ptr %tok_p acquire, align 8\n\
  %tok = inttoptr i64 %tok_i64 to ptr\n\
  %has_rfn = icmp ne ptr %rfn, null\n\
  %has_tok = icmp ne ptr %tok, null\n\
  %can_call = and i1 %has_rfn, %has_tok\n\
  br i1 %can_call, label %call_req, label %unlock\n\
call_req:\n\
  call void %rfn(ptr %tok)\n\
  br label %unlock\n\
unlock:\n\
  store atomic i32 0, ptr %drv_p release, align 4\n\
  br label %done\n\
done:\n\
  ret void\n\
}\n\
define i1 @__zil_areg_poll_close(ptr %frame) {\n\
entry:\n\
  %is_null = icmp eq ptr %frame, null\n\
  br i1 %is_null, label %ret_ready, label %try_lock\n\
try_lock:\n\
  %drv_p = getelementptr inbounds %__zil_areg_hdr, ptr %frame, i32 0, i32 1\n\
  %lock_res = cmpxchg ptr %drv_p, i32 0, i32 1 acq_rel acquire\n\
  %got_lock = extractvalue { i32, i1 } %lock_res, 1\n\
  br i1 %got_lock, label %ensure_req, label %ret_pending\n\
ensure_req:\n\
  %sent_p = getelementptr inbounds %__zil_areg_hdr, ptr %frame, i32 0, i32 2\n\
  %sent = load i32, ptr %sent_p\n\
  %need_send = icmp eq i32 %sent, 0\n\
  br i1 %need_send, label %do_req, label %check_gate\n\
do_req:\n\
  call void @__zil_areg_mark_closing(ptr %frame)\n\
  store i32 1, ptr %sent_p\n\
  %rfn_p = getelementptr inbounds %__zil_areg_hdr, ptr %frame, i32 0, i32 4\n\
  %rfn = load ptr, ptr %rfn_p\n\
  %tok_p0 = getelementptr inbounds %__zil_areg_hdr, ptr %frame, i32 0, i32 3\n\
  %tok0_i64 = load atomic i64, ptr %tok_p0 acquire, align 8\n\
  %tok0 = inttoptr i64 %tok0_i64 to ptr\n\
  %has_rfn = icmp ne ptr %rfn, null\n\
  %has_tok0 = icmp ne ptr %tok0, null\n\
  %can_req = and i1 %has_rfn, %has_tok0\n\
  br i1 %can_req, label %call_req, label %check_gate\n\
call_req:\n\
  call void %rfn(ptr %tok0)\n\
  br label %check_gate\n\
check_gate:\n\
  %gate_p = getelementptr inbounds %__zil_areg_hdr, ptr %frame, i32 0, i32 0\n\
  %g = load atomic i32, ptr %gate_p acquire, align 4\n\
  %is_closed = icmp eq i32 %g, 4\n\
  br i1 %is_closed, label %unlock_ready, label %check_idle\n\
check_idle:\n\
  %is_idle = icmp eq i32 %g, 2\n\
  br i1 %is_idle, label %call_drain, label %unlock_pending\n\
call_drain:\n\
  %pfn_p = getelementptr inbounds %__zil_areg_hdr, ptr %frame, i32 0, i32 5\n\
  %pfn = load ptr, ptr %pfn_p\n\
  %tok_p = getelementptr inbounds %__zil_areg_hdr, ptr %frame, i32 0, i32 3\n\
  %tok_i64 = load atomic i64, ptr %tok_p acquire, align 8\n\
  %tok = inttoptr i64 %tok_i64 to ptr\n\
  %has_pfn = icmp ne ptr %pfn, null\n\
  %has_tok = icmp ne ptr %tok, null\n\
  %can_drain = and i1 %has_pfn, %has_tok\n\
  br i1 %can_drain, label %do_drain, label %cleanup\n\
do_drain:\n\
  %rc = call i32 %pfn(ptr %tok)\n\
  %rc_pend = icmp eq i32 %rc, 0\n\
  br i1 %rc_pend, label %unlock_pending, label %check_rc_one\n\
check_rc_one:\n\
  %rc_done = icmp eq i32 %rc, 1\n\
  br i1 %rc_done, label %recheck_idle, label %invalid_drain\n\
recheck_idle:\n\
  %g2 = load atomic i32, ptr %gate_p acquire, align 4\n\
  %still_idle = icmp eq i32 %g2, 2\n\
  br i1 %still_idle, label %cleanup, label %unlock_pending\n\
cleanup:\n\
  %dfn_p = getelementptr inbounds %__zil_areg_hdr, ptr %frame, i32 0, i32 6\n\
  %dfn = load ptr, ptr %dfn_p\n\
  %has_dfn = icmp ne ptr %dfn, null\n\
  br i1 %has_dfn, label %call_drop, label %publish_closed\n\
call_drop:\n\
  store ptr null, ptr %dfn_p\n\
  call void %dfn(ptr %frame)\n\
  br label %publish_closed\n\
publish_closed:\n\
  store atomic i32 4, ptr %gate_p release, align 4\n\
  br label %unlock_ready\n\
invalid_drain:\n\
  store atomic i32 0, ptr %drv_p release, align 4\n\
  call void @abort()\n\
  unreachable\n\
unlock_pending:\n\
  store atomic i32 0, ptr %drv_p release, align 4\n\
  br label %ret_pending\n\
unlock_ready:\n\
  store atomic i32 0, ptr %drv_p release, align 4\n\
  br label %ret_ready\n\
ret_pending:\n\
  ret i1 0\n\
ret_ready:\n\
  ret i1 1\n\
}\n\
define void @__zil_areg_finish_destroy(ptr %frame) {\n\
entry:\n\
  %is_null = icmp eq ptr %frame, null\n\
  br i1 %is_null, label %done, label %check\n\
check:\n\
  %gate_p = getelementptr inbounds %__zil_areg_hdr, ptr %frame, i32 0, i32 0\n\
  %g = load atomic i32, ptr %gate_p acquire, align 4\n\
  %is_closed = icmp eq i32 %g, 4\n\
  br i1 %is_closed, label %do_free, label %early_abort\n\
early_abort:\n\
  call void @abort()\n\
  unreachable\n\
do_free:\n\
  call void @free(ptr %frame)\n\
  br label %done\n\
done:\n\
  ret void\n\
}\n",
        );
        if is_windows {
            runtime_defs.push_str(
                "define void @__zil_areg_drop_frame(ptr %frame) {\n\
entry:\n\
  %is_null = icmp eq ptr %frame, null\n\
  br i1 %is_null, label %done, label %loop\n\
loop:\n\
  %ready = call i1 @__zil_areg_poll_close(ptr %frame)\n\
  br i1 %ready, label %finish, label %maybe_yield\n\
maybe_yield:\n\
  %cur = load ptr, ptr @__zil_current_task\n\
  %in_task = icmp ne ptr %cur, null\n\
  br i1 %in_task, label %yield_task, label %loop\n\
yield_task:\n\
  %st_p = getelementptr inbounds %__zil_task_hdr, ptr %cur, i32 0, i32 0\n\
  %cur_st = load i32, ptr %st_p\n\
  %is_cl = icmp eq i32 %cur_st, 4\n\
  %next_st = select i1 %is_cl, i32 4, i32 1\n\
  store i32 %next_st, ptr %st_p\n\
  %cctx = getelementptr inbounds %__zil_task_hdr, ptr %cur, i32 0, i32 2\n\
  %cfib = load ptr, ptr %cctx\n\
  call void @SwitchToFiber(ptr %cfib)\n\
  br label %loop\n\
finish:\n\
  call void @__zil_areg_finish_destroy(ptr %frame)\n\
  br label %done\n\
done:\n\
  ret void\n\
}\n",
            );
        } else {
            runtime_defs.push_str(
                "define void @__zil_areg_drop_frame(ptr %frame) {\n\
entry:\n\
  %is_null = icmp eq ptr %frame, null\n\
  br i1 %is_null, label %done, label %loop\n\
loop:\n\
  %ready = call i1 @__zil_areg_poll_close(ptr %frame)\n\
  br i1 %ready, label %finish, label %maybe_yield\n\
maybe_yield:\n\
  %cur = load ptr, ptr @__zil_current_task\n\
  %in_task = icmp ne ptr %cur, null\n\
  br i1 %in_task, label %yield_task, label %loop\n\
yield_task:\n\
  %st_p = getelementptr inbounds %__zil_task_hdr, ptr %cur, i32 0, i32 0\n\
  %cur_st = load i32, ptr %st_p\n\
  %is_cl = icmp eq i32 %cur_st, 4\n\
  %next_st = select i1 %is_cl, i32 4, i32 1\n\
  store i32 %next_st, ptr %st_p\n\
  %cctx = getelementptr inbounds %__zil_task_hdr, ptr %cur, i32 0, i32 2\n\
  %wctx = getelementptr inbounds %__zil_task_hdr, ptr %cur, i32 0, i32 3\n\
  %sc = call i32 @swapcontext(ptr %wctx, ptr %cctx)\n\
  %sc_ok = icmp eq i32 %sc, 0\n\
  br i1 %sc_ok, label %loop, label %sys_abort\n\
sys_abort:\n\
  call void @abort()\n\
  unreachable\n\
finish:\n\
  call void @__zil_areg_finish_destroy(ptr %frame)\n\
  br label %done\n\
done:\n\
  ret void\n\
}\n",
            );
        }
    }
    let _ = runtime_linkage;
    if reach.uses_bytes_grow {
        header.push_str(
            "define internal i1 @__zil_bytes_grow(ptr %h, i64 %cap) noinline {\n\
entry:\n\
  %is_zero = icmp eq i64 %cap, 0\n\
  %doubled = shl i64 %cap, 1\n\
  %new_cap = select i1 %is_zero, i64 8, i64 %doubled\n\
  %data_slot = getelementptr inbounds { ptr, i64, i64 }, ptr %h, i32 0, i32 0\n\
  %old_data = load ptr, ptr %data_slot\n\
  %new_data = call ptr @realloc(ptr %old_data, i64 %new_cap)\n\
  %ok_alloc = icmp ne ptr %new_data, null\n\
  br i1 %ok_alloc, label %commit, label %fail\n\
commit:\n\
  store ptr %new_data, ptr %data_slot\n\
  %cap_slot = getelementptr inbounds { ptr, i64, i64 }, ptr %h, i32 0, i32 2\n\
  store i64 %new_cap, ptr %cap_slot\n\
  ret i1 1\n\
fail:\n\
  ret i1 0\n\
}\n\n",
        );
    }
    header.push_str(
        "@__zil_fmt_i64 = private unnamed_addr constant [6 x i8] c\"%lld\\0A\\00\", align 1\n\n",
    );

    for (idx, bytes) in module.statics.iter().enumerate() {
        let escaped = escape_llvm_bytes(bytes);
        let _ = writeln!(
            header,
            "@__zil_str_{idx} = private unnamed_addr constant [{} x i8] c\"{escaped}\", align 1",
            bytes.len()
        );
    }
    if !module.statics.is_empty() {
        header.push('\n');
    }

    let mut foreign_header = String::new();
    let mut emitted_foreigns = std::collections::BTreeSet::new();
    for f in &module.foreigns {
        if !emitted_foreigns.insert(f.symbol.as_str()) {
            continue;
        }
        if f.symbol == "zil_glibc_errno_exchange" {
            let errno_fn = if is_windows {
                "_errno"
            } else {
                "__errno_location"
            };
            let _ = writeln!(foreign_header, "declare ptr @{errno_fn}()");
            let _ = writeln!(
                foreign_header,
                "define internal i32 @zil_glibc_errno_exchange(i32 %next) {{"
            );
            let _ = writeln!(foreign_header, "entry:");
            let _ = writeln!(foreign_header, "  %p = call ptr @{errno_fn}()");
            let _ = writeln!(foreign_header, "  %old = load i32, ptr %p");
            let _ = writeln!(foreign_header, "  store i32 %next, ptr %p");
            let _ = writeln!(foreign_header, "  ret i32 %old");
            let _ = writeln!(foreign_header, "}}");
            continue;
        }
        if f.symbol == "zil_process_arg_count" {
            let _ = writeln!(
                foreign_header,
                "define internal i64 @zil_process_arg_count() {{"
            );
            let _ = writeln!(foreign_header, "entry:");
            let _ = writeln!(foreign_header, "  %argv = load ptr, ptr @zil_process_argv");
            let _ = writeln!(foreign_header, "  %is_null = icmp eq ptr %argv, null");
            let _ = writeln!(
                foreign_header,
                "  br i1 %is_null, label %zero, label %check_argc"
            );
            let _ = writeln!(foreign_header, "check_argc:");
            let _ = writeln!(foreign_header, "  %c = load i32, ptr @zil_process_argc");
            let _ = writeln!(foreign_header, "  %neg = icmp slt i32 %c, 0");
            let _ = writeln!(foreign_header, "  br i1 %neg, label %zero, label %ok");
            let _ = writeln!(foreign_header, "ok:");
            let _ = writeln!(foreign_header, "  %ext = sext i32 %c to i64");
            let _ = writeln!(foreign_header, "  ret i64 %ext");
            let _ = writeln!(foreign_header, "zero:");
            let _ = writeln!(foreign_header, "  ret i64 0");
            let _ = writeln!(foreign_header, "}}");
            continue;
        }
        if f.symbol == "zil_process_arg_ptr" {
            let _ = writeln!(
                foreign_header,
                "define internal ptr @zil_process_arg_ptr(i64 %idx) {{"
            );
            let _ = writeln!(foreign_header, "entry:");
            let _ = writeln!(foreign_header, "  %argv = load ptr, ptr @zil_process_argv");
            let _ = writeln!(foreign_header, "  %is_null = icmp eq ptr %argv, null");
            let _ = writeln!(
                foreign_header,
                "  br i1 %is_null, label %ret_null, label %check_bounds"
            );
            let _ = writeln!(foreign_header, "check_bounds:");
            let _ = writeln!(foreign_header, "  %c = load i32, ptr @zil_process_argc");
            let _ = writeln!(foreign_header, "  %c64 = sext i32 %c to i64");
            let _ = writeln!(foreign_header, "  %neg = icmp slt i64 %idx, 0");
            let _ = writeln!(
                foreign_header,
                "  br i1 %neg, label %ret_null, label %check_upper"
            );
            let _ = writeln!(foreign_header, "check_upper:");
            let _ = writeln!(foreign_header, "  %in_range = icmp slt i64 %idx, %c64");
            let _ = writeln!(
                foreign_header,
                "  br i1 %in_range, label %load_arg, label %ret_null"
            );
            let _ = writeln!(foreign_header, "load_arg:");
            let _ = writeln!(
                foreign_header,
                "  %slot = getelementptr inbounds ptr, ptr %argv, i64 %idx"
            );
            let _ = writeln!(foreign_header, "  %arg = load ptr, ptr %slot");
            let _ = writeln!(foreign_header, "  ret ptr %arg");
            let _ = writeln!(foreign_header, "ret_null:");
            let _ = writeln!(foreign_header, "  ret ptr null");
            let _ = writeln!(foreign_header, "}}");
            continue;
        }
        let ret = llvm_ty(&module.tys, f.result)?;
        let mut params = Vec::with_capacity(f.params.len());
        for &p in &f.params {
            let t = llvm_ty(&module.tys, p)?;
            if t != "void" {
                params.push(t);
            }
        }
        let _ = writeln!(
            foreign_header,
            "declare {ret} @{}({})",
            sanitize_symbol(&f.symbol),
            params.join(", ")
        );
    }
    if !module.foreigns.is_empty() {
        foreign_header.push('\n');
    }

    let mut units = Vec::with_capacity(actual_units);
    for unit_idx in 0..actual_units {
        let mut out =
            String::with_capacity(header.len() + runtime_defs.len() + foreign_header.len() + 4096);
        out.push_str(&header);
        if unit_idx == 0 {
            out.push_str(&runtime_defs);
        } else {
            out.push_str(&runtime_decls);
        }
        out.push_str(&foreign_header);

        // Emit declarations for functions owned by other units.
        if actual_units > 1 {
            for (idx, func) in module.functions.iter().enumerate() {
                if idx % actual_units != unit_idx {
                    emit_function_decl(module, func, &syms[idx], &mut out)?;
                }
            }
            if unit_idx != 0 {
                for (i, dimpl) in module.dyn_impls.iter().enumerate() {
                    let n = dimpl.methods.len() + 1;
                    let _ = writeln!(out, "@__zil_vtable_{i} = external constant [{n} x ptr]");
                }
            }
            out.push('\n');
        }

        let fn_cfg = FnEmitConfig {
            is_windows,
            internalize: single_exe,
            emit_cancel_clear: reach.uses_tasks,
        };
        // Emit definitions for functions owned by this unit.
        for (idx, func) in module.functions.iter().enumerate() {
            if idx % actual_units == unit_idx && reach.fns[idx] {
                emit_function(
                    module,
                    FnId::from_index(idx),
                    func,
                    &syms[idx],
                    fn_cfg,
                    &mut out,
                )?;
                out.push('\n');
            }
        }

        // Emit dyn Trait vtables, drop shims, and slot thunks in unit 0.
        if unit_idx == 0 && !module.dyn_impls.is_empty() {
            emit_dyn_vtables(
                module,
                &syms,
                is_windows,
                single_exe,
                &reach.dyn_impls,
                &mut out,
            )?;
        }

        // Emit C `main()` wrapper in unit 0.
        if unit_idx == 0
            && emit_c_main
            && let Some((_, ref sym, res_ty)) = main_fn
        {
            let ret = llvm_ty(&module.tys, res_ty)?;
            let mut main_extra_defs = String::new();
            out.push_str(
                "define i32 @main(i32 %argc, ptr %argv) {\nentry:\n  store i32 %argc, ptr @zil_process_argc\n  store ptr %argv, ptr @zil_process_argv\n",
            );
            if let Some(TyKind::ErrorUnion { ok, err }) = module.tys.get(res_ty) {
                let ok_ty = *ok;
                let err_ty = *err;
                let ok_llvm = llvm_ty(&module.tys, ok_ty)?;
                let ret_storage = llvm_storage_ty(&module.tys, res_ty)?;
                let _ = writeln!(out, "  %r_slot = alloca {ret_storage}");
                let _ = writeln!(out, "  %r = call {ret} @{sym}()");
                let _ = writeln!(out, "  store {ret} %r, ptr %r_slot");
                let _ = writeln!(out, "  %tag = extractvalue {ret} %r, 0");
                out.push_str("  br i1 %tag, label %ok_bb, label %err_bb\n");
                out.push_str("ok_bb:\n");
                if ok_llvm == "void" {
                    out.push_str("  ret i32 0\n");
                } else if ok_llvm == "i32" {
                    let _ = writeln!(out, "  %v = extractvalue {ret} %r, 1");
                    if custom_entry {
                        out.push_str("  %ext = sext i32 %v to i64\n");
                        out.push_str(
                            "  call i32 (ptr, ...) @printf(ptr @__zil_fmt_i64, i64 %ext)\n",
                        );
                        out.push_str("  ret i32 0\n");
                    } else {
                        out.push_str("  ret i32 %v\n");
                    }
                } else if ok_llvm == "i64" {
                    let _ = writeln!(out, "  %v = extractvalue {ret} %r, 1");
                    if custom_entry {
                        out.push_str("  call i32 (ptr, ...) @printf(ptr @__zil_fmt_i64, i64 %v)\n");
                        out.push_str("  ret i32 0\n");
                    } else {
                        out.push_str("  %t = trunc i64 %v to i32\n  ret i32 %t\n");
                    }
                } else {
                    out.push_str("  ret i32 0\n");
                }
                out.push_str("err_bb:\n");
                let _ = writeln!(
                    out,
                    "  %err_ptr = getelementptr inbounds {ret_storage}, ptr %r_slot, i32 0, i32 2"
                );
                let empty_body = Body {
                    locals: Vec::new(),
                    blocks: Vec::new(),
                    param_count: 0,
                    result: TyId(0),
                    captures: Vec::new(),
                    closure: None,
                    drop_flags: Vec::new(),
                    param_env: zil_ir::body::ParamEnv::default(),
                };
                {
                    let mut emitter = FnEmitter {
                        module,
                        fn_id: FnId(0),
                        body: &empty_body,
                        is_windows,
                        emit_cancel_clear: false,
                        out: &mut out,
                        entry_allocas: None,
                        extra_defs: String::new(),
                        tmp: 0,
                    };
                    emitter.emit_drop_ptr("%err_ptr", err_ty)?;
                    main_extra_defs = emitter.extra_defs;
                }
                out.push_str("  ret i32 1\n");
            } else if ret == "void" {
                let _ = writeln!(out, "  call void @{sym}()");
                out.push_str("  ret i32 0\n");
            } else if ret == "i32" {
                let _ = writeln!(out, "  %r = call i32 @{sym}()");
                if custom_entry {
                    out.push_str("  %ext = sext i32 %r to i64\n");
                    out.push_str("  call i32 (ptr, ...) @printf(ptr @__zil_fmt_i64, i64 %ext)\n");
                    out.push_str("  ret i32 0\n");
                } else {
                    out.push_str("  ret i32 %r\n");
                }
            } else if ret == "i64" {
                let _ = writeln!(out, "  %r = call i64 @{sym}()");
                if custom_entry {
                    out.push_str("  call i32 (ptr, ...) @printf(ptr @__zil_fmt_i64, i64 %r)\n");
                    out.push_str("  ret i32 0\n");
                } else {
                    out.push_str("  %t = trunc i64 %r to i32\n  ret i32 %t\n");
                }
            } else if ret == "i1" {
                let _ = writeln!(out, "  %r = call i1 @{sym}()");
                out.push_str("  %t = zext i1 %r to i32\n  ret i32 %t\n");
            } else {
                let _ = writeln!(out, "  %r = call {ret} @{sym}()");
                out.push_str("  ret i32 0\n");
            }
            out.push_str("}\n");
            if !main_extra_defs.is_empty() {
                out.push_str(&main_extra_defs);
            }
        }

        units.push(out);
    }

    Ok(units)
}

fn fn_symbol(id: FnId, f: &Function) -> String {
    format!("__zil_fn_{}_{}", id.0, sanitize_symbol(&f.name))
}

fn sanitize_symbol(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn escape_llvm_bytes(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 3);
    for &b in bytes {
        let _ = write!(out, "\\{b:02X}");
    }
    out
}

fn llvm_int_ty(i: IntTy) -> &'static str {
    match i {
        IntTy::I8 | IntTy::U8 => "i8",
        IntTy::I16 | IntTy::U16 => "i16",
        IntTy::I32 | IntTy::U32 => "i32",
        IntTy::I64 | IntTy::U64 | IntTy::Isize | IntTy::Usize => "i64",
    }
}

fn is_inline_managed(tys: &TyTable, inner: TyId) -> bool {
    matches!(
        tys.get(inner),
        Some(
            TyKind::Adt { .. } | TyKind::Opaque(OpaqueTy::ByteBuffer | OpaqueTy::OwnedStore { .. })
        )
    )
}

fn llvm_ty(tys: &TyTable, id: TyId) -> Result<String, BodyEmitError> {
    let kind = tys
        .get(id)
        .ok_or_else(|| BodyEmitError::new(format!("missing type {id:?}")))?;
    Ok(match kind {
        TyKind::Unit | TyKind::Never => "void".into(),
        TyKind::Bool => "i1".into(),
        TyKind::Int(i) => llvm_int_ty(*i).into(),
        TyKind::Float(FloatTy::F32) => "float".into(),
        TyKind::Float(FloatTy::F64) => "double".into(),
        TyKind::Ref { pointee, .. } => {
            if matches!(tys.get(*pointee), Some(TyKind::Slice(_))) {
                "{ ptr, i64 }".into()
            } else {
                "ptr".into()
            }
        }
        TyKind::Managed(inner) => match tys.get(*inner) {
            Some(TyKind::Adt { .. }) => llvm_storage_ty(tys, *inner)?,
            Some(TyKind::Opaque(OpaqueTy::ByteBuffer | OpaqueTy::OwnedStore { .. })) => {
                "{ ptr, i64, i64 }".into()
            }
            Some(TyKind::OwnedDyn { .. } | TyKind::Dyn { .. }) => "{ ptr, ptr }".into(),
            _ => "ptr".into(),
        },
        TyKind::Dyn { .. } | TyKind::OwnedDyn { .. } => "{ ptr, ptr }".into(),
        TyKind::Opaque(OpaqueTy::SyncCell(_)) => "i64".into(),
        TyKind::Desc(_) | TyKind::FnPtr { .. } | TyKind::Opaque(_) => "ptr".into(),
        TyKind::Slice(_) => "{ ptr, i64 }".into(),
        TyKind::Array { element, length } => {
            let elem = llvm_storage_ty(tys, *element)?;
            format!("[{length} x {elem}]")
        }
        TyKind::Tuple(items) => {
            if items.is_empty() {
                "void".into()
            } else {
                let mut fields = Vec::with_capacity(items.len());
                for &item in items {
                    fields.push(llvm_storage_ty(tys, item)?);
                }
                format!("{{ {} }}", fields.join(", "))
            }
        }
        TyKind::ErrorUnion { ok, err } => {
            let ok_t = llvm_storage_ty(tys, *ok)?;
            let err_t = llvm_storage_ty(tys, *err)?;
            format!("{{ i1, {ok_t}, {err_t} }}")
        }
        TyKind::Adt { adt, args } => {
            let def = tys
                .get_adt(*adt)
                .ok_or_else(|| BodyEmitError::new(format!("missing ADT {adt:?}")))?;
            match def.kind {
                AdtKind::Record | AdtKind::Package | AdtKind::Resource => {
                    let fields = def
                        .variants
                        .first()
                        .map(|v| &v.fields)
                        .ok_or_else(|| BodyEmitError::new("ADT has no variant"))?;
                    if fields.is_empty() {
                        "{ i8 }".into()
                    } else {
                        let mut parts = Vec::with_capacity(fields.len());
                        for f in fields {
                            let fty = tys.subst_lookup(f.ty, *args).unwrap_or(f.ty);
                            parts.push(llvm_storage_ty(tys, fty)?);
                        }
                        format!("{{ {} }}", parts.join(", "))
                    }
                }
                AdtKind::Enum => {
                    // Tagged layout: `{ i32, <v0>, <v1>, ... }` so every variant
                    // payload has a statically typed GEP slot without bitcast UB.
                    let mut parts = vec!["i32".to_string()];
                    for v in &def.variants {
                        if v.fields.is_empty() {
                            parts.push("i8".into());
                        } else {
                            let mut vfields = Vec::with_capacity(v.fields.len());
                            for f in &v.fields {
                                let fty = tys.subst_lookup(f.ty, *args).unwrap_or(f.ty);
                                vfields.push(llvm_storage_ty(tys, fty)?);
                            }
                            parts.push(format!("{{ {} }}", vfields.join(", ")));
                        }
                    }
                    format!("{{ {} }}", parts.join(", "))
                }
            }
        }
        TyKind::Closure { def, args } => {
            let cdef = tys
                .closure(*def)
                .ok_or_else(|| BodyEmitError::new(format!("missing closure {def:?}")))?;
            if cdef.captures.is_empty() {
                "{ i8 }".into()
            } else {
                let mut parts = Vec::with_capacity(cdef.captures.len());
                for &cap in &cdef.captures {
                    let cty = tys.subst_lookup(cap, *args).unwrap_or(cap);
                    parts.push(llvm_storage_ty(tys, cty)?);
                }
                format!("{{ {} }}", parts.join(", "))
            }
        }
        TyKind::Param(_) => {
            return Err(BodyEmitError::new(
                "unmonomorphized generic type parameter in body IR",
            ));
        }
    })
}

fn llvm_storage_ty(tys: &TyTable, id: TyId) -> Result<String, BodyEmitError> {
    let t = llvm_ty(tys, id)?;
    if t == "void" { Ok("i8".into()) } else { Ok(t) }
}

struct FnEmitter<'a> {
    module: &'a Module,
    fn_id: FnId,
    body: &'a Body,
    is_windows: bool,
    emit_cancel_clear: bool,
    out: &'a mut String,
    entry_allocas: Option<&'a mut String>,
    extra_defs: String,
    tmp: u32,
}

impl<'a> FnEmitter<'a> {
    fn fresh(&mut self) -> String {
        let id = self.tmp;
        self.tmp += 1;
        format!("%t{id}")
    }

    fn alloc_entry(&mut self, storage_ty: &str) -> String {
        let slot = self.fresh();
        if let Some(entry) = &mut self.entry_allocas {
            let _ = writeln!(entry, "  {slot} = alloca {storage_ty}");
        } else {
            let _ = writeln!(self.out, "  {slot} = alloca {storage_ty}");
        }
        slot
    }

    fn tys(&self) -> &'a TyTable {
        &self.module.tys
    }

    fn local_ty(&self, local: Local) -> Result<TyId, BodyEmitError> {
        self.body
            .locals
            .get(local.index())
            .map(|l| l.ty)
            .ok_or_else(|| BodyEmitError::new(format!("unknown local {local:?}")))
    }

    fn step_ty(&self, base: TyId, step: Projection) -> Result<TyId, BodyEmitError> {
        let tys = self.tys();
        match step {
            Projection::Field(FieldRef {
                adt,
                variant,
                field,
                args,
            }) => tys
                .field_ty(adt, variant, field, args)
                .ok_or_else(|| BodyEmitError::new("missing field type")),
            Projection::TupleField(i) => match tys.get(base) {
                Some(TyKind::Tuple(items)) => items
                    .get(i as usize)
                    .copied()
                    .ok_or_else(|| BodyEmitError::new("tuple field out of range")),
                _ => Err(BodyEmitError::new("tuple field on non-tuple")),
            },
            Projection::ErrorArm(ok) => match tys.get(base) {
                Some(TyKind::ErrorUnion { ok: o, err: e }) => Ok(if ok { *o } else { *e }),
                _ => Err(BodyEmitError::new("error arm on non-error-union")),
            },
            Projection::Index(_) | Projection::ConstIndex(_) => match tys.get(base) {
                Some(TyKind::Array { element, .. } | TyKind::Slice(element)) => Ok(*element),
                _ => Err(BodyEmitError::new("index on non-array/slice")),
            },
            Projection::Deref => match tys.get(base) {
                Some(
                    TyKind::Ref { pointee, .. } | TyKind::Managed(pointee) | TyKind::Desc(pointee),
                ) => Ok(*pointee),
                _ => Err(BodyEmitError::new("deref on non-pointer")),
            },
        }
    }

    /// Emits a pointer to the storage of `place` and returns `(ptr_ssa, place_ty_id)`.
    fn emit_place_ptr(&mut self, place: &Place) -> Result<(String, TyId), BodyEmitError> {
        let mut ty = match place.root {
            PlaceRoot::Local(l) => self.local_ty(l)?,
            PlaceRoot::Capture(i) => *self
                .body
                .captures
                .get(i as usize)
                .ok_or_else(|| BodyEmitError::new(format!("unknown capture {i}")))?,
        };
        let mut ptr = match place.root {
            PlaceRoot::Local(l) => format!("%l{}", l.0),
            PlaceRoot::Capture(i) => {
                let Some(cid) = self.body.closure else {
                    return Err(BodyEmitError::new("capture root outside closure"));
                };
                let cdef = self
                    .tys()
                    .closure(cid)
                    .ok_or_else(|| BodyEmitError::new("missing closure def"))?;
                let mut fields = Vec::with_capacity(cdef.captures.len());
                for &ct in &cdef.captures {
                    fields.push(llvm_storage_ty(self.tys(), ct)?);
                }
                let env_ty = if fields.is_empty() {
                    "{ i8 }".to_string()
                } else {
                    format!("{{ {} }}", fields.join(", "))
                };
                let gep = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {gep} = getelementptr inbounds {env_ty}, ptr %__env, i32 0, i32 {i}"
                );
                gep
            }
        };

        for step in &place.projections {
            let base_llvm = llvm_storage_ty(self.tys(), ty)?;
            match *step {
                Projection::Deref => {
                    let is_transparent = match self.tys().get(ty) {
                        Some(TyKind::Ref { pointee, .. }) => {
                            matches!(self.tys().get(*pointee), Some(TyKind::Slice(_)))
                        }
                        Some(TyKind::Managed(pointee)) => is_inline_managed(self.tys(), *pointee),
                        _ => false,
                    };
                    if is_transparent {
                        // Inline Managed or slice ref already resides at `ptr`.
                        ty = self.step_ty(ty, *step)?;
                    } else {
                        let loaded = self.fresh();
                        let _ = writeln!(self.out, "  {loaded} = load ptr, ptr {ptr}");
                        ptr = loaded;
                        ty = self.step_ty(ty, *step)?;
                    }
                }
                Projection::Field(FieldRef {
                    adt,
                    variant,
                    field,
                    ..
                }) => {
                    let def = self
                        .tys()
                        .get_adt(adt)
                        .ok_or_else(|| BodyEmitError::new("missing ADT"))?;
                    match def.kind {
                        AdtKind::Record | AdtKind::Package | AdtKind::Resource => {
                            let gep = self.fresh();
                            let _ = writeln!(
                                self.out,
                                "  {gep} = getelementptr inbounds {base_llvm}, ptr {ptr}, i32 0, i32 {}",
                                field.0
                            );
                            ptr = gep;
                        }
                        AdtKind::Enum => {
                            let v_slot = variant.0 + 1;
                            let gep = self.fresh();
                            let _ = writeln!(
                                self.out,
                                "  {gep} = getelementptr inbounds {base_llvm}, ptr {ptr}, i32 0, i32 {v_slot}, i32 {}",
                                field.0
                            );
                            ptr = gep;
                        }
                    }
                    ty = self.step_ty(ty, *step)?;
                }
                Projection::TupleField(i) => {
                    let gep = self.fresh();
                    let _ = writeln!(
                        self.out,
                        "  {gep} = getelementptr inbounds {base_llvm}, ptr {ptr}, i32 0, i32 {i}"
                    );
                    ptr = gep;
                    ty = self.step_ty(ty, *step)?;
                }
                Projection::ErrorArm(ok) => {
                    let slot = if ok { 1 } else { 2 };
                    let gep = self.fresh();
                    let _ = writeln!(
                        self.out,
                        "  {gep} = getelementptr inbounds {base_llvm}, ptr {ptr}, i32 0, i32 {slot}"
                    );
                    ptr = gep;
                    ty = self.step_ty(ty, *step)?;
                }
                Projection::ConstIndex(idx) => {
                    let elem_ty = self.step_ty(ty, *step)?;
                    let elem_llvm = llvm_storage_ty(self.tys(), elem_ty)?;
                    if matches!(self.tys().get(ty), Some(TyKind::Slice(_))) {
                        let data_ptr_slot = self.fresh();
                        let _ = writeln!(
                            self.out,
                            "  {data_ptr_slot} = getelementptr inbounds {{ ptr, i64 }}, ptr {ptr}, i32 0, i32 0"
                        );
                        let data_ptr = self.fresh();
                        let _ = writeln!(self.out, "  {data_ptr} = load ptr, ptr {data_ptr_slot}");
                        let gep = self.fresh();
                        let _ = writeln!(
                            self.out,
                            "  {gep} = getelementptr inbounds {elem_llvm}, ptr {data_ptr}, i64 {idx}"
                        );
                        ptr = gep;
                    } else {
                        let gep = self.fresh();
                        let _ = writeln!(
                            self.out,
                            "  {gep} = getelementptr inbounds {base_llvm}, ptr {ptr}, i64 0, i64 {idx}"
                        );
                        ptr = gep;
                    }
                    ty = elem_ty;
                }
                Projection::Index(idx_local) => {
                    let elem_ty = self.step_ty(ty, *step)?;
                    let elem_llvm = llvm_storage_ty(self.tys(), elem_ty)?;
                    let idx_val = self.fresh();
                    let _ = writeln!(self.out, "  {idx_val} = load i64, ptr %l{}", idx_local.0);
                    let tid = self.tmp;
                    self.tmp += 1;
                    let l_ok = format!("idx_ok_{tid}");
                    let l_oob = format!("idx_oob_{tid}");
                    if matches!(self.tys().get(ty), Some(TyKind::Slice(_))) {
                        let len_slot = self.fresh();
                        let _ = writeln!(
                            self.out,
                            "  {len_slot} = getelementptr inbounds {{ ptr, i64 }}, ptr {ptr}, i32 0, i32 1"
                        );
                        let len_val = self.fresh();
                        let _ = writeln!(self.out, "  {len_val} = load i64, ptr {len_slot}");
                        let in_bounds = self.fresh();
                        let _ = writeln!(
                            self.out,
                            "  {in_bounds} = icmp ult i64 {idx_val}, {len_val}"
                        );
                        let _ = writeln!(
                            self.out,
                            "  br i1 {in_bounds}, label %{l_ok}, label %{l_oob}"
                        );
                        let _ = writeln!(self.out, "{l_oob}:");
                        let _ = writeln!(self.out, "  call void @abort()");
                        let _ = writeln!(self.out, "  unreachable");
                        let _ = writeln!(self.out, "{l_ok}:");
                        let data_ptr_slot = self.fresh();
                        let _ = writeln!(
                            self.out,
                            "  {data_ptr_slot} = getelementptr inbounds {{ ptr, i64 }}, ptr {ptr}, i32 0, i32 0"
                        );
                        let data_ptr = self.fresh();
                        let _ = writeln!(self.out, "  {data_ptr} = load ptr, ptr {data_ptr_slot}");
                        let gep = self.fresh();
                        let _ = writeln!(
                            self.out,
                            "  {gep} = getelementptr inbounds {elem_llvm}, ptr {data_ptr}, i64 {idx_val}"
                        );
                        ptr = gep;
                    } else {
                        if let Some(TyKind::Array { length, .. }) = self.tys().get(ty) {
                            let len_val = *length as i64;
                            let in_bounds = self.fresh();
                            let _ = writeln!(
                                self.out,
                                "  {in_bounds} = icmp ult i64 {idx_val}, {len_val}"
                            );
                            let _ = writeln!(
                                self.out,
                                "  br i1 {in_bounds}, label %{l_ok}, label %{l_oob}"
                            );
                            let _ = writeln!(self.out, "{l_oob}:");
                            let _ = writeln!(self.out, "  call void @abort()");
                            let _ = writeln!(self.out, "  unreachable");
                            let _ = writeln!(self.out, "{l_ok}:");
                        }
                        let gep = self.fresh();
                        let _ = writeln!(
                            self.out,
                            "  {gep} = getelementptr inbounds {base_llvm}, ptr {ptr}, i64 0, i64 {idx_val}"
                        );
                        ptr = gep;
                    }
                    ty = elem_ty;
                }
            }
        }
        Ok((ptr, ty))
    }

    fn emit_operand(
        &mut self,
        op: &Operand,
    ) -> Result<(String, String, Option<TyId>), BodyEmitError> {
        match op {
            Operand::Copy(p) | Operand::Move(p) => {
                let (ptr, ty) = self.emit_place_ptr(p)?;
                let lt = llvm_ty(self.tys(), ty)?;
                if lt == "void" {
                    Ok(("0".into(), "void".into(), Some(ty)))
                } else {
                    let val = self.fresh();
                    let _ = writeln!(self.out, "  {val} = load {lt}, ptr {ptr}");
                    Ok((val, lt, Some(ty)))
                }
            }
            Operand::Const(c) => match c {
                ConstValue::Unit => Ok(("0".into(), "void".into(), None)),
                ConstValue::Bool(b) => Ok((if *b { "1" } else { "0" }.into(), "i1".into(), None)),
                ConstValue::Int { ty, bits } => {
                    let lt = llvm_int_ty(*ty);
                    let val = match ty {
                        IntTy::I8 => (*bits as i8).to_string(),
                        IntTy::I16 => (*bits as i16).to_string(),
                        IntTy::I32 => (*bits as i32).to_string(),
                        IntTy::I64 | IntTy::Isize => (*bits as i64).to_string(),
                        IntTy::U8 => (*bits as u8).to_string(),
                        IntTy::U16 => (*bits as u16).to_string(),
                        IntTy::U32 => (*bits as u32).to_string(),
                        IntTy::U64 | IntTy::Usize => (*bits as u64).to_string(),
                    };
                    Ok((val, lt.into(), None))
                }
                ConstValue::Float { ty, bits } => match ty {
                    FloatTy::F32 => {
                        let f = f32::from_bits(*bits as u32) as f64;
                        let hex = f.to_bits();
                        Ok((format!("0x{hex:016X}"), "float".into(), None))
                    }
                    FloatTy::F64 => Ok((format!("0x{bits:016X}"), "double".into(), None)),
                },
            },
            Operand::Pooled(_) => Err(BodyEmitError::new("pooled constants not yet emitted")),
        }
    }

    fn emit_instruction(&mut self, kind: &InstructionKind) -> Result<(), BodyEmitError> {
        match kind {
            InstructionKind::Assign { dest, value } => self.emit_assign(dest, value),
            InstructionKind::Drop { place } => {
                let (ptr, ty) = self.emit_place_ptr(place)?;
                self.emit_drop_ptr(&ptr, ty)
            }
            InstructionKind::Replace { place, value, old } => {
                let (p_ptr, p_ty) = self.emit_place_ptr(place)?;
                let lt = llvm_ty(self.tys(), p_ty)?;
                let (new_val, _, _) = self.emit_operand(value)?;
                if lt != "void" {
                    let old_val = self.fresh();
                    let _ = writeln!(self.out, "  {old_val} = load {lt}, ptr {p_ptr}");
                    let _ = writeln!(self.out, "  store {lt} {old_val}, ptr %l{}", old.0);
                    let _ = writeln!(self.out, "  store {lt} {new_val}, ptr {p_ptr}");
                }
                Ok(())
            }
            InstructionKind::StorageLive(_)
            | InstructionKind::StorageDead(_)
            | InstructionKind::ScopeEnter(_)
            | InstructionKind::ScopeExit {
                scope: _,
                exit:
                    ScopeExit::Fallthrough
                    | ScopeExit::Return
                    | ScopeExit::Error
                    | ScopeExit::Break
                    | ScopeExit::Continue,
            }
            | InstructionKind::RegisterDefer { .. } => Ok(()),
        }
    }

    fn emit_drop_ptr(&mut self, ptr: &str, ty: TyId) -> Result<(), BodyEmitError> {
        let Some(kind) = self.tys().get(ty).cloned() else {
            return Ok(());
        };
        match kind {
            TyKind::Managed(inner) => {
                let inline = is_inline_managed(self.tys(), inner);
                let handle = if inline {
                    ptr.to_string()
                } else {
                    let h = self.fresh();
                    let _ = writeln!(self.out, "  {h} = load ptr, ptr {ptr}");
                    h
                };
                match self.tys().get(inner).cloned() {
                    Some(TyKind::Adt { adt, .. }) => {
                        if let Some(drop_fn_id) = self.module.drop_fn(adt) {
                            let sym =
                                fn_symbol(drop_fn_id, &self.module.functions[drop_fn_id.index()]);
                            let _ = writeln!(self.out, "  call void @{sym}(ptr {handle})");
                        }
                    }
                    Some(TyKind::Opaque(OpaqueTy::ByteBuffer)) => {
                        let data_slot = self.fresh();
                        let _ = writeln!(
                            self.out,
                            "  {data_slot} = getelementptr inbounds {{ ptr, i64, i64 }}, ptr {handle}, i32 0, i32 0"
                        );
                        let data = self.fresh();
                        let _ = writeln!(self.out, "  {data} = load ptr, ptr {data_slot}");
                        let _ = writeln!(self.out, "  call void @free(ptr {data})");
                    }
                    Some(TyKind::Opaque(OpaqueTy::OwnedStore { element: elem })) => {
                        let data_slot = self.fresh();
                        let _ = writeln!(
                            self.out,
                            "  {data_slot} = getelementptr inbounds {{ ptr, i64, i64 }}, ptr {handle}, i32 0, i32 0"
                        );
                        let data = self.fresh();
                        let _ = writeln!(self.out, "  {data} = load ptr, ptr {data_slot}");
                        let len_slot = self.fresh();
                        let _ = writeln!(
                            self.out,
                            "  {len_slot} = getelementptr inbounds {{ ptr, i64, i64 }}, ptr {handle}, i32 0, i32 1"
                        );
                        let len = self.fresh();
                        let _ = writeln!(self.out, "  {len} = load i64, ptr {len_slot}");
                        let elem_llvm = llvm_storage_ty(self.tys(), elem)?;
                        let idx_slot = self.alloc_entry("i64");
                        let _ = writeln!(self.out, "  store i64 0, ptr {idx_slot}");
                        let head = self.fresh();
                        let body_bb = self.fresh();
                        let done = self.fresh();
                        let (hl, bl, dl) = (&head[1..], &body_bb[1..], &done[1..]);
                        let _ = writeln!(self.out, "  br label %{hl}");
                        let _ = writeln!(self.out, "{hl}:");
                        let cur = self.fresh();
                        let _ = writeln!(self.out, "  {cur} = load i64, ptr {idx_slot}");
                        let cond = self.fresh();
                        let _ = writeln!(self.out, "  {cond} = icmp slt i64 {cur}, {len}");
                        let _ = writeln!(self.out, "  br i1 {cond}, label %{bl}, label %{dl}");
                        let _ = writeln!(self.out, "{bl}:");
                        let elem_ptr = self.fresh();
                        let _ = writeln!(
                            self.out,
                            "  {elem_ptr} = getelementptr inbounds {elem_llvm}, ptr {data}, i64 {cur}"
                        );
                        self.emit_drop_ptr(&elem_ptr, elem)?;
                        let next = self.fresh();
                        let _ = writeln!(self.out, "  {next} = add i64 {cur}, 1");
                        let _ = writeln!(self.out, "  store i64 {next}, ptr {idx_slot}");
                        let _ = writeln!(self.out, "  br label %{hl}");
                        let _ = writeln!(self.out, "{dl}:");
                        let _ = writeln!(self.out, "  call void @free(ptr {data})");
                    }
                    Some(TyKind::Opaque(OpaqueTy::Mutex { payload: elem })) => {
                        if !self.is_windows {
                            let rc = self.fresh();
                            let ok = self.fresh();
                            let tid = self.tmp;
                            self.tmp += 1;
                            let l_ok = format!("md_ok_{tid}");
                            let l_ab = format!("md_ab_{tid}");
                            let _ = writeln!(
                                self.out,
                                "  {rc} = call i32 @pthread_mutex_destroy(ptr {handle})"
                            );
                            let _ = writeln!(self.out, "  {ok} = icmp eq i32 {rc}, 0");
                            let _ =
                                writeln!(self.out, "  br i1 {ok}, label %{l_ok}, label %{l_ab}");
                            let _ = writeln!(self.out, "{l_ab}:");
                            let _ = writeln!(self.out, "  call void @abort()");
                            let _ = writeln!(self.out, "  unreachable");
                            let _ = writeln!(self.out, "{l_ok}:");
                        }
                        let pay_ptr = self.fresh();
                        let _ = writeln!(
                            self.out,
                            "  {pay_ptr} = getelementptr inbounds i8, ptr {handle}, i64 64"
                        );
                        self.emit_drop_ptr(&pay_ptr, elem)?;
                    }
                    Some(TyKind::Opaque(OpaqueTy::MutexGuard { .. })) => {
                        if self.is_windows {
                            let _ = writeln!(
                                self.out,
                                "  call void @ReleaseSRWLockExclusive(ptr {handle})"
                            );
                        } else {
                            let rc = self.fresh();
                            let ok = self.fresh();
                            let tid = self.tmp;
                            self.tmp += 1;
                            let l_ok = format!("mgu_ok_{tid}");
                            let l_ab = format!("mgu_ab_{tid}");
                            let _ = writeln!(
                                self.out,
                                "  {rc} = call i32 @pthread_mutex_unlock(ptr {handle})"
                            );
                            let _ = writeln!(self.out, "  {ok} = icmp eq i32 {rc}, 0");
                            let _ =
                                writeln!(self.out, "  br i1 {ok}, label %{l_ok}, label %{l_ab}");
                            let _ = writeln!(self.out, "{l_ab}:");
                            let _ = writeln!(self.out, "  call void @abort()");
                            let _ = writeln!(self.out, "  unreachable");
                            let _ = writeln!(self.out, "{l_ok}:");
                        }
                        return Ok(());
                    }
                    Some(TyKind::Opaque(OpaqueTy::Task { .. })) => {
                        let _ =
                            writeln!(self.out, "  call void @__zil_task_drop_frame(ptr {handle})");
                        return Ok(());
                    }
                    Some(TyKind::Opaque(OpaqueTy::Registration { .. })) => {
                        let _ =
                            writeln!(self.out, "  call void @__zil_reg_drop_frame(ptr {handle})");
                        return Ok(());
                    }
                    Some(TyKind::Opaque(OpaqueTy::AsyncRegistration { .. })) => {
                        let _ =
                            writeln!(self.out, "  call void @__zil_areg_drop_frame(ptr {handle})");
                        return Ok(());
                    }
                    Some(TyKind::OwnedDyn { .. }) => {
                        self.emit_owned_dyn_drop(ptr);
                        return Ok(());
                    }
                    Some(
                        TyKind::Unit
                        | TyKind::Bool
                        | TyKind::Never
                        | TyKind::Int(_)
                        | TyKind::Float(_)
                        | TyKind::Tuple(_)
                        | TyKind::Array { .. }
                        | TyKind::Slice(_)
                        | TyKind::Ref { .. }
                        | TyKind::Managed(_)
                        | TyKind::Desc(_)
                        | TyKind::ErrorUnion { .. }
                        | TyKind::FnPtr { .. }
                        | TyKind::Closure { .. }
                        | TyKind::Dyn { .. }
                        | TyKind::Param(_)
                        | TyKind::Opaque(
                            OpaqueTy::SyncCell { .. }
                            | OpaqueTy::ForeignHandle
                            | OpaqueTy::CWideString
                            | OpaqueTy::ForeignCallback { .. },
                        ),
                    )
                    | None => {}
                }
                if !inline {
                    let _ = writeln!(self.out, "  call void @free(ptr {handle})");
                }
                Ok(())
            }
            TyKind::Adt { adt, args } => {
                let Some(def) = self.tys().get_adt(adt) else {
                    return Ok(());
                };
                match def.kind {
                    AdtKind::Package | AdtKind::Record => {
                        let base_llvm = llvm_storage_ty(self.tys(), ty)?;
                        if let Some(v) = def.variants.first() {
                            for (fi, f) in v.fields.iter().enumerate() {
                                let fty = self.tys().subst_lookup(f.ty, args).unwrap_or(f.ty);
                                let gep = self.fresh();
                                let _ = writeln!(
                                    self.out,
                                    "  {gep} = getelementptr inbounds {base_llvm}, ptr {ptr}, i32 0, i32 {fi}"
                                );
                                self.emit_drop_ptr(&gep, fty)?;
                            }
                        }
                        Ok(())
                    }
                    AdtKind::Enum => {
                        let base_llvm = llvm_storage_ty(self.tys(), ty)?;
                        let variants: Vec<(usize, Vec<TyId>)> = def
                            .variants
                            .iter()
                            .enumerate()
                            .filter(|(_, v)| !v.fields.is_empty())
                            .map(|(vi, v)| {
                                let ftys = v
                                    .fields
                                    .iter()
                                    .map(|f| self.tys().subst_lookup(f.ty, args).unwrap_or(f.ty))
                                    .collect();
                                (vi, ftys)
                            })
                            .collect();
                        let mut variant_drops: Vec<(usize, String)> = Vec::new();
                        for (vi, ftys) in variants {
                            let mut v_parts = Vec::with_capacity(ftys.len());
                            for &fty in &ftys {
                                v_parts.push(llvm_storage_ty(self.tys(), fty)?);
                            }
                            let v_llvm = format!("{{ {} }}", v_parts.join(", "));
                            let vp = self.fresh();
                            let saved_len = self.out.len();
                            let mut any_drop = false;
                            for (fi, fty) in ftys.into_iter().enumerate() {
                                let fgep = self.fresh();
                                let f_saved = self.out.len();
                                let res = self.emit_drop_ptr(&fgep, fty);
                                let f_ir = self.out.split_off(f_saved);
                                res?;
                                if !f_ir.is_empty() {
                                    any_drop = true;
                                    let _ = writeln!(
                                        self.out,
                                        "  {fgep} = getelementptr inbounds {v_llvm}, ptr {vp}, i32 0, i32 {fi}"
                                    );
                                    self.out.push_str(&f_ir);
                                }
                            }
                            let v_ir = self.out.split_off(saved_len);
                            if any_drop {
                                let mut full_v_ir = String::new();
                                let _ = writeln!(
                                    full_v_ir,
                                    "  {vp} = getelementptr inbounds {base_llvm}, ptr {ptr}, i32 0, i32 {}",
                                    1 + vi
                                );
                                full_v_ir.push_str(&v_ir);
                                variant_drops.push((vi, full_v_ir));
                            }
                        }
                        if variant_drops.is_empty() {
                            return Ok(());
                        }
                        let tag_p = self.fresh();
                        let tag = self.fresh();
                        let done_bb = self.fresh();
                        let done_lbl = done_bb[1..].to_string();
                        let _ = writeln!(
                            self.out,
                            "  {tag_p} = getelementptr inbounds {base_llvm}, ptr {ptr}, i32 0, i32 0"
                        );
                        let _ = writeln!(self.out, "  {tag} = load i32, ptr {tag_p}");
                        let mut arm_lbls = Vec::with_capacity(variant_drops.len());
                        for _ in &variant_drops {
                            let bb = self.fresh();
                            arm_lbls.push(bb[1..].to_string());
                        }
                        let _ = writeln!(self.out, "  switch i32 {tag}, label %{done_lbl} [");
                        for ((vi, _), lbl) in variant_drops.iter().zip(&arm_lbls) {
                            let _ = writeln!(self.out, "    i32 {vi}, label %{lbl}");
                        }
                        let _ = writeln!(self.out, "  ]");
                        for ((_, v_ir), lbl) in variant_drops.into_iter().zip(&arm_lbls) {
                            let _ = writeln!(self.out, "{lbl}:");
                            self.out.push_str(&v_ir);
                            let _ = writeln!(self.out, "  br label %{done_lbl}");
                        }
                        let _ = writeln!(self.out, "{done_lbl}:");
                        Ok(())
                    }
                    AdtKind::Resource => Ok(()),
                }
            }
            TyKind::Tuple(elems) => {
                let base_llvm = llvm_storage_ty(self.tys(), ty)?;
                for (i, ety) in elems.into_iter().enumerate().rev() {
                    let gep = self.fresh();
                    let _ = writeln!(
                        self.out,
                        "  {gep} = getelementptr inbounds {base_llvm}, ptr {ptr}, i32 0, i32 {i}"
                    );
                    self.emit_drop_ptr(&gep, ety)?;
                }
                Ok(())
            }
            TyKind::Closure { def, args } => {
                let Some(cdef) = self.tys().closure(def).cloned() else {
                    return Ok(());
                };
                let base_llvm = llvm_storage_ty(self.tys(), ty)?;
                for (i, &cty) in cdef.captures.iter().enumerate() {
                    let fty = self.tys().subst_lookup(cty, args).unwrap_or(cty);
                    let gep = self.fresh();
                    let _ = writeln!(
                        self.out,
                        "  {gep} = getelementptr inbounds {base_llvm}, ptr {ptr}, i32 0, i32 {i}"
                    );
                    self.emit_drop_ptr(&gep, fty)?;
                }
                Ok(())
            }
            TyKind::ErrorUnion { ok, err } => {
                let base_llvm = llvm_storage_ty(self.tys(), ty)?;
                let tag_ptr = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {tag_ptr} = getelementptr inbounds {base_llvm}, ptr {ptr}, i32 0, i32 0"
                );
                let tag = self.fresh();
                let _ = writeln!(self.out, "  {tag} = load i1, ptr {tag_ptr}");
                let ok_bb = self.fresh();
                let err_bb = self.fresh();
                let done_bb = self.fresh();
                let (ol, el, dl) = (&ok_bb[1..], &err_bb[1..], &done_bb[1..]);
                let _ = writeln!(self.out, "  br i1 {tag}, label %{ol}, label %{el}");
                let _ = writeln!(self.out, "{ol}:");
                let ok_ptr = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {ok_ptr} = getelementptr inbounds {base_llvm}, ptr {ptr}, i32 0, i32 1"
                );
                self.emit_drop_ptr(&ok_ptr, ok)?;
                let _ = writeln!(self.out, "  br label %{dl}");
                let _ = writeln!(self.out, "{el}:");
                let err_ptr = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {err_ptr} = getelementptr inbounds {base_llvm}, ptr {ptr}, i32 0, i32 2"
                );
                self.emit_drop_ptr(&err_ptr, err)?;
                let _ = writeln!(self.out, "  br label %{dl}");
                let _ = writeln!(self.out, "{dl}:");
                Ok(())
            }
            TyKind::Opaque(OpaqueTy::MutexGuard { .. }) => {
                let handle = self.fresh();
                let _ = writeln!(self.out, "  {handle} = load ptr, ptr {ptr}");
                if self.is_windows {
                    let _ = writeln!(
                        self.out,
                        "  call void @ReleaseSRWLockExclusive(ptr {handle})"
                    );
                } else {
                    let rc = self.fresh();
                    let ok = self.fresh();
                    let tid = self.tmp;
                    self.tmp += 1;
                    let l_ok = format!("mgo_ok_{tid}");
                    let l_ab = format!("mgo_ab_{tid}");
                    let _ = writeln!(
                        self.out,
                        "  {rc} = call i32 @pthread_mutex_unlock(ptr {handle})"
                    );
                    let _ = writeln!(self.out, "  {ok} = icmp eq i32 {rc}, 0");
                    let _ = writeln!(self.out, "  br i1 {ok}, label %{l_ok}, label %{l_ab}");
                    let _ = writeln!(self.out, "{l_ab}:");
                    let _ = writeln!(self.out, "  call void @abort()");
                    let _ = writeln!(self.out, "  unreachable");
                    let _ = writeln!(self.out, "{l_ok}:");
                }
                Ok(())
            }
            TyKind::OwnedDyn { .. } => {
                self.emit_owned_dyn_drop(ptr);
                Ok(())
            }
            TyKind::Unit
            | TyKind::Bool
            | TyKind::Never
            | TyKind::Int(_)
            | TyKind::Float(_)
            | TyKind::Array { .. }
            | TyKind::Slice(_)
            | TyKind::Ref { .. }
            | TyKind::Desc(_)
            | TyKind::FnPtr { .. }
            | TyKind::Dyn { .. }
            | TyKind::Param(_)
            | TyKind::Opaque(_) => Ok(()),
        }
    }

    fn emit_owned_dyn_drop(&mut self, ptr: &str) {
        let fat = self.fresh();
        let data = self.fresh();
        let vt = self.fresh();
        let nonnull = self.fresh();
        let drop_fn = self.fresh();
        let lbl_drop = format!("od_drop_{}", self.tmp);
        let lbl_done = format!("od_drop_done_{}", self.tmp);
        self.tmp += 1;
        let _ = writeln!(self.out, "  {fat} = load {{ ptr, ptr }}, ptr {ptr}");
        let _ = writeln!(self.out, "  {data} = extractvalue {{ ptr, ptr }} {fat}, 0");
        let _ = writeln!(self.out, "  {vt} = extractvalue {{ ptr, ptr }} {fat}, 1");
        let _ = writeln!(self.out, "  {nonnull} = icmp ne ptr {data}, null");
        let _ = writeln!(
            self.out,
            "  br i1 {nonnull}, label %{lbl_drop}, label %{lbl_done}"
        );
        let _ = writeln!(self.out, "{lbl_drop}:");
        let _ = writeln!(self.out, "  {drop_fn} = load ptr, ptr {vt}");
        let _ = writeln!(self.out, "  call void {drop_fn}(ptr {data})");
        let _ = writeln!(self.out, "  br label %{lbl_done}");
        let _ = writeln!(self.out, "{lbl_done}:");
    }

    fn emit_assign(&mut self, dest: &Place, value: &Rvalue) -> Result<(), BodyEmitError> {
        let (d_ptr, d_ty) = self.emit_place_ptr(dest)?;
        let d_llvm = llvm_ty(self.tys(), d_ty)?;
        match value {
            Rvalue::Use(op) => {
                let (v, lt, _) = self.emit_operand(op)?;
                if lt != "void" && d_llvm != "void" {
                    let _ = writeln!(self.out, "  store {d_llvm} {v}, ptr {d_ptr}");
                }
                Ok(())
            }
            Rvalue::Ref { place, .. } => {
                let (p_ptr, p_ty) = self.emit_place_ptr(place)?;
                if matches!(self.tys().get(p_ty), Some(TyKind::Slice(_))) {
                    let fat = self.fresh();
                    let _ = writeln!(self.out, "  {fat} = load {{ ptr, i64 }}, ptr {p_ptr}");
                    let _ = writeln!(self.out, "  store {{ ptr, i64 }} {fat}, ptr {d_ptr}");
                } else {
                    let _ = writeln!(self.out, "  store ptr {p_ptr}, ptr {d_ptr}");
                }
                Ok(())
            }
            Rvalue::SliceRef {
                place, start, end, ..
            } => {
                let (p_ptr, p_ty) = self.emit_place_ptr(place)?;
                let (base_data, base_len, elem_ty) = match self.tys().get(p_ty) {
                    Some(TyKind::Array { element, length }) => {
                        (p_ptr, (*length as i64).to_string(), *element)
                    }
                    Some(TyKind::Slice(element)) => {
                        let dp_slot = self.fresh();
                        let _ = writeln!(
                            self.out,
                            "  {dp_slot} = getelementptr inbounds {{ ptr, i64 }}, ptr {p_ptr}, i32 0, i32 0"
                        );
                        let dp = self.fresh();
                        let _ = writeln!(self.out, "  {dp} = load ptr, ptr {dp_slot}");
                        let len_slot = self.fresh();
                        let _ = writeln!(
                            self.out,
                            "  {len_slot} = getelementptr inbounds {{ ptr, i64 }}, ptr {p_ptr}, i32 0, i32 1"
                        );
                        let len = self.fresh();
                        let _ = writeln!(self.out, "  {len} = load i64, ptr {len_slot}");
                        (dp, len, *element)
                    }
                    _ => return Err(BodyEmitError::new("SliceRef on non-array/slice")),
                };
                let start_v = match start {
                    Some(op) => self.emit_operand(op)?.0,
                    None => "0".into(),
                };
                let end_v = match end {
                    Some(op) => self.emit_operand(op)?.0,
                    None => base_len.clone(),
                };
                let tid = self.tmp;
                self.tmp += 1;
                let l_ok = format!("slice_ok_{tid}");
                let l_oob = format!("slice_oob_{tid}");
                let ok1 = self.fresh();
                let _ = writeln!(self.out, "  {ok1} = icmp ule i64 {start_v}, {end_v}");
                let ok2 = self.fresh();
                let _ = writeln!(self.out, "  {ok2} = icmp ule i64 {end_v}, {base_len}");
                let ok = self.fresh();
                let _ = writeln!(self.out, "  {ok} = and i1 {ok1}, {ok2}");
                let _ = writeln!(self.out, "  br i1 {ok}, label %{l_ok}, label %{l_oob}");
                let _ = writeln!(self.out, "{l_oob}:");
                let _ = writeln!(self.out, "  call void @abort()");
                let _ = writeln!(self.out, "  unreachable");
                let _ = writeln!(self.out, "{l_ok}:");
                let elem_llvm = llvm_storage_ty(self.tys(), elem_ty)?;
                let new_ptr = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {new_ptr} = getelementptr inbounds {elem_llvm}, ptr {base_data}, i64 {start_v}"
                );
                let new_len = self.fresh();
                let _ = writeln!(self.out, "  {new_len} = sub i64 {end_v}, {start_v}");
                let s0 = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {s0} = insertvalue {{ ptr, i64 }} undef, ptr {new_ptr}, 0"
                );
                let s1 = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {s1} = insertvalue {{ ptr, i64 }} {s0}, i64 {new_len}, 1"
                );
                let _ = writeln!(self.out, "  store {{ ptr, i64 }} {s1}, ptr {d_ptr}");
                Ok(())
            }
            Rvalue::Describe { place } => {
                let (p_ptr, p_ty) = self.emit_place_ptr(place)?;
                let inline = matches!(
                    self.tys().get(p_ty),
                    Some(TyKind::Managed(inner)) if is_inline_managed(self.tys(), *inner)
                );
                if inline {
                    let _ = writeln!(self.out, "  store ptr {p_ptr}, ptr {d_ptr}");
                } else {
                    let handle = self.fresh();
                    let _ = writeln!(self.out, "  {handle} = load ptr, ptr {p_ptr}");
                    let _ = writeln!(self.out, "  store ptr {handle}, ptr {d_ptr}");
                }
                Ok(())
            }
            Rvalue::Aggregate { kind, operands } => {
                if d_llvm == "void" {
                    return Ok(());
                }
                match kind {
                    AggregateKind::Tuple | AggregateKind::Closure { .. } => {
                        for (i, op) in operands.iter().enumerate() {
                            let (v, lt, _) = self.emit_operand(op)?;
                            let gep = self.fresh();
                            let _ = writeln!(
                                self.out,
                                "  {gep} = getelementptr inbounds {d_llvm}, ptr {d_ptr}, i32 0, i32 {i}"
                            );
                            if lt == "void" {
                                let _ = writeln!(self.out, "  store i8 0, ptr {gep}");
                            } else {
                                let _ = writeln!(self.out, "  store {lt} {v}, ptr {gep}");
                            }
                        }
                        Ok(())
                    }
                    AggregateKind::Array { .. } => {
                        for (i, op) in operands.iter().enumerate() {
                            let (v, lt, _) = self.emit_operand(op)?;
                            let gep = self.fresh();
                            let _ = writeln!(
                                self.out,
                                "  {gep} = getelementptr inbounds {d_llvm}, ptr {d_ptr}, i64 0, i64 {i}"
                            );
                            if lt == "void" {
                                let _ = writeln!(self.out, "  store i8 0, ptr {gep}");
                            } else {
                                let _ = writeln!(self.out, "  store {lt} {v}, ptr {gep}");
                            }
                        }
                        Ok(())
                    }
                    AggregateKind::Adt { adt, variant, .. } => {
                        let def = self
                            .tys()
                            .get_adt(*adt)
                            .ok_or_else(|| BodyEmitError::new("missing ADT"))?;
                        match def.kind {
                            AdtKind::Record | AdtKind::Package | AdtKind::Resource => {
                                for (i, op) in operands.iter().enumerate() {
                                    let (v, lt, _) = self.emit_operand(op)?;
                                    let gep = self.fresh();
                                    let _ = writeln!(
                                        self.out,
                                        "  {gep} = getelementptr inbounds {d_llvm}, ptr {d_ptr}, i32 0, i32 {i}"
                                    );
                                    if lt == "void" {
                                        let _ = writeln!(self.out, "  store i8 0, ptr {gep}");
                                    } else {
                                        let _ = writeln!(self.out, "  store {lt} {v}, ptr {gep}");
                                    }
                                }
                            }
                            AdtKind::Enum => {
                                let tag_ptr = self.fresh();
                                let _ = writeln!(
                                    self.out,
                                    "  {tag_ptr} = getelementptr inbounds {d_llvm}, ptr {d_ptr}, i32 0, i32 0"
                                );
                                let _ =
                                    writeln!(self.out, "  store i32 {}, ptr {tag_ptr}", variant.0);
                                let v_slot = variant.0 + 1;
                                for (i, op) in operands.iter().enumerate() {
                                    let (v, lt, _) = self.emit_operand(op)?;
                                    let gep = self.fresh();
                                    let _ = writeln!(
                                        self.out,
                                        "  {gep} = getelementptr inbounds {d_llvm}, ptr {d_ptr}, i32 0, i32 {v_slot}, i32 {i}"
                                    );
                                    if lt == "void" {
                                        let _ = writeln!(self.out, "  store i8 0, ptr {gep}");
                                    } else {
                                        let _ = writeln!(self.out, "  store {lt} {v}, ptr {gep}");
                                    }
                                }
                            }
                        }
                        Ok(())
                    }
                }
            }
            Rvalue::ErrorUnion { ok, value, .. } => {
                let tag_ptr = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {tag_ptr} = getelementptr inbounds {d_llvm}, ptr {d_ptr}, i32 0, i32 0"
                );
                let _ = writeln!(
                    self.out,
                    "  store i1 {}, ptr {tag_ptr}",
                    if *ok { 1 } else { 0 }
                );
                let (v, lt, _) = self.emit_operand(value)?;
                let slot = if *ok { 1 } else { 2 };
                let val_ptr = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {val_ptr} = getelementptr inbounds {d_llvm}, ptr {d_ptr}, i32 0, i32 {slot}"
                );
                if lt == "void" {
                    let _ = writeln!(self.out, "  store i8 0, ptr {val_ptr}");
                } else {
                    let _ = writeln!(self.out, "  store {lt} {v}, ptr {val_ptr}");
                }
                Ok(())
            }
            Rvalue::Unary { op, operand } => {
                let (v, lt, _) = self.emit_operand(operand)?;
                let res = self.fresh();
                match op {
                    UnOp::Not => {
                        let _ = writeln!(self.out, "  {res} = xor {lt} {v}, -1");
                    }
                    UnOp::Neg | UnOp::WrappingNeg => {
                        if lt == "float" || lt == "double" {
                            let _ = writeln!(self.out, "  {res} = fneg {lt} {v}");
                        } else {
                            let _ = writeln!(self.out, "  {res} = sub {lt} 0, {v}");
                        }
                    }
                }
                let _ = writeln!(self.out, "  store {d_llvm} {res}, ptr {d_ptr}");
                Ok(())
            }
            Rvalue::Binary { op, lhs, rhs } => {
                let (lv, lt, l_ty) = self.emit_operand(lhs)?;
                let (rv, _, _) = self.emit_operand(rhs)?;
                let signed = match lhs {
                    Operand::Const(ConstValue::Int { ty, .. }) => ty.signed(),
                    Operand::Copy(_)
                    | Operand::Move(_)
                    | Operand::Const(_)
                    | Operand::Pooled(_) => l_ty
                        .and_then(|t| self.tys().get(t))
                        .is_some_and(|k| matches!(k, TyKind::Int(i) if i.signed())),
                };
                let is_float = lt == "float" || lt == "double";
                let res = self.fresh();
                let instr = match op {
                    BinOp::Add | BinOp::WrappingAdd => {
                        if is_float {
                            "fadd"
                        } else {
                            "add"
                        }
                    }
                    BinOp::Sub | BinOp::WrappingSub => {
                        if is_float {
                            "fsub"
                        } else {
                            "sub"
                        }
                    }
                    BinOp::Mul | BinOp::WrappingMul => {
                        if is_float {
                            "fmul"
                        } else {
                            "mul"
                        }
                    }
                    BinOp::Div => {
                        if is_float {
                            "fdiv"
                        } else if signed {
                            "sdiv"
                        } else {
                            "udiv"
                        }
                    }
                    BinOp::Rem => {
                        if is_float {
                            "frem"
                        } else if signed {
                            "srem"
                        } else {
                            "urem"
                        }
                    }
                    BinOp::BitAnd => "and",
                    BinOp::BitOr => "or",
                    BinOp::BitXor => "xor",
                    BinOp::Shl => "shl",
                    BinOp::Shr => {
                        if signed {
                            "ashr"
                        } else {
                            "lshr"
                        }
                    }
                    BinOp::Eq => {
                        if is_float {
                            "fcmp oeq"
                        } else {
                            "icmp eq"
                        }
                    }
                    BinOp::Ne => {
                        if is_float {
                            "fcmp one"
                        } else {
                            "icmp ne"
                        }
                    }
                    BinOp::Lt => {
                        if is_float {
                            "fcmp olt"
                        } else if signed {
                            "icmp slt"
                        } else {
                            "icmp ult"
                        }
                    }
                    BinOp::Le => {
                        if is_float {
                            "fcmp ole"
                        } else if signed {
                            "icmp sle"
                        } else {
                            "icmp ule"
                        }
                    }
                    BinOp::Gt => {
                        if is_float {
                            "fcmp ogt"
                        } else if signed {
                            "icmp sgt"
                        } else {
                            "icmp ugt"
                        }
                    }
                    BinOp::Ge => {
                        if is_float {
                            "fcmp oge"
                        } else if signed {
                            "icmp sge"
                        } else {
                            "icmp uge"
                        }
                    }
                };
                let _ = writeln!(self.out, "  {res} = {instr} {lt} {lv}, {rv}");
                let _ = writeln!(self.out, "  store {d_llvm} {res}, ptr {d_ptr}");
                Ok(())
            }
            Rvalue::Convert { operand, to } => {
                let (v, from_lt, from_ty) = self.emit_operand(operand)?;
                let to_lt = llvm_ty(self.tys(), *to)?;
                if from_lt == to_lt {
                    let _ = writeln!(self.out, "  store {to_lt} {v}, ptr {d_ptr}");
                    return Ok(());
                }
                let res = self.fresh();
                let from_int = match operand {
                    Operand::Const(ConstValue::Int { ty, .. }) => Some(*ty),
                    Operand::Copy(_)
                    | Operand::Move(_)
                    | Operand::Const(_)
                    | Operand::Pooled(_) => from_ty.and_then(|t| match self.tys().get(t) {
                        Some(TyKind::Int(i)) => Some(*i),
                        _ => None,
                    }),
                };
                let from_float = match operand {
                    Operand::Const(ConstValue::Float { ty, .. }) => Some(*ty),
                    Operand::Copy(_)
                    | Operand::Move(_)
                    | Operand::Const(_)
                    | Operand::Pooled(_) => from_ty.and_then(|t| {
                        if let Some(TyKind::Float(f)) = self.tys().get(t) {
                            Some(*f)
                        } else {
                            None
                        }
                    }),
                };
                let to_kind = self.tys().get(*to);
                let cast = match (from_int, from_float, to_kind) {
                    (Some(a), _, Some(TyKind::Int(b))) => {
                        let ab = a.bits(64);
                        let bb = b.bits(64);
                        if ab < bb {
                            if a.signed() { "sext" } else { "zext" }
                        } else {
                            "trunc"
                        }
                    }
                    (Some(a), _, Some(TyKind::Float(_))) => {
                        if a.signed() {
                            "sitofp"
                        } else {
                            "uitofp"
                        }
                    }
                    (_, Some(_), Some(TyKind::Int(b))) => {
                        if b.signed() {
                            "fptosi"
                        } else {
                            "fptoui"
                        }
                    }
                    (_, Some(FloatTy::F32), Some(TyKind::Float(FloatTy::F64))) => "fpext",
                    (_, Some(FloatTy::F64), Some(TyKind::Float(FloatTy::F32))) => "fptrunc",
                    _ => "bitcast",
                };
                let _ = writeln!(self.out, "  {res} = {cast} {from_lt} {v} to {to_lt}");
                let _ = writeln!(self.out, "  store {to_lt} {res}, ptr {d_ptr}");
                Ok(())
            }
            Rvalue::Discriminant(place) => {
                let (p_ptr, p_ty) = self.emit_place_ptr(place)?;
                let p_llvm = llvm_storage_ty(self.tys(), p_ty)?;
                let tag_ptr = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {tag_ptr} = getelementptr inbounds {p_llvm}, ptr {p_ptr}, i32 0, i32 0"
                );
                let tag = self.fresh();
                let _ = writeln!(self.out, "  {tag} = load i32, ptr {tag_ptr}");
                let _ = writeln!(self.out, "  store i32 {tag}, ptr {d_ptr}");
                Ok(())
            }
            Rvalue::ErrorArm(place) => {
                let (p_ptr, p_ty) = self.emit_place_ptr(place)?;
                let p_llvm = llvm_storage_ty(self.tys(), p_ty)?;
                let tag_ptr = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {tag_ptr} = getelementptr inbounds {p_llvm}, ptr {p_ptr}, i32 0, i32 0"
                );
                let tag = self.fresh();
                let _ = writeln!(self.out, "  {tag} = load i1, ptr {tag_ptr}");
                let _ = writeln!(self.out, "  store i1 {tag}, ptr {d_ptr}");
                Ok(())
            }
            Rvalue::Len(place) => {
                let (p_ptr, p_ty) = self.emit_place_ptr(place)?;
                match self.tys().get(p_ty) {
                    Some(TyKind::Array { length, .. }) => {
                        let _ = writeln!(self.out, "  store i64 {length}, ptr {d_ptr}");
                    }
                    Some(TyKind::Slice(_)) => {
                        let len_ptr = self.fresh();
                        let _ = writeln!(
                            self.out,
                            "  {len_ptr} = getelementptr inbounds {{ ptr, i64 }}, ptr {p_ptr}, i32 0, i32 1"
                        );
                        let len = self.fresh();
                        let _ = writeln!(self.out, "  {len} = load i64, ptr {len_ptr}");
                        let _ = writeln!(self.out, "  store i64 {len}, ptr {d_ptr}");
                    }
                    _ => return Err(BodyEmitError::new("Len on non-array/slice")),
                }
                Ok(())
            }
            Rvalue::FnRef(fid) => {
                let sym = fn_symbol(*fid, &self.module.functions[fid.index()]);
                let _ = writeln!(self.out, "  store ptr @{sym}, ptr {d_ptr}");
                Ok(())
            }
            Rvalue::ForeignFnRef(fid) => {
                let sym = sanitize_symbol(&self.module.foreigns[fid.index()].symbol);
                let _ = writeln!(self.out, "  store ptr @{sym}, ptr {d_ptr}");
                Ok(())
            }
            Rvalue::StaticUtf(sid) => {
                let len = self.module.statics[sid.index()].len();
                let s0 = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {s0} = insertvalue {{ ptr, i64 }} undef, ptr @__zil_str_{}, 0",
                    sid.0
                );
                let s1 = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {s1} = insertvalue {{ ptr, i64 }} {s0}, i64 {len}, 1"
                );
                let _ = writeln!(self.out, "  store {{ ptr, i64 }} {s1}, ptr {d_ptr}");
                Ok(())
            }
        }
    }

    fn emit_terminator(&mut self, term: &Terminator) -> Result<(), BodyEmitError> {
        match term {
            Terminator::Goto { target } => {
                let _ = writeln!(self.out, "  br label %bb{}", target.0);
                Ok(())
            }
            Terminator::Switch {
                discr,
                arms,
                otherwise,
            } => {
                let (v, lt, _) = self.emit_operand(discr)?;
                let _ = writeln!(self.out, "  switch {lt} {v}, label %bb{} [", otherwise.0);
                for (val, target) in arms {
                    let _ = writeln!(self.out, "    {lt} {val}, label %bb{}", target.0);
                }
                self.out.push_str("  ]\n");
                Ok(())
            }
            Terminator::Return { value } => {
                let (v, lt, _) = self.emit_operand(value)?;
                if self.emit_cancel_clear {
                    self.out
                        .push_str("  store i1 false, ptr @__zil_task_cancel_trap\n");
                }
                if lt == "void" {
                    self.out.push_str("  ret void\n");
                } else {
                    let _ = writeln!(self.out, "  ret {lt} {v}");
                }
                Ok(())
            }
            Terminator::Call {
                callee,
                args,
                dest,
                target,
                cancel,
            } => {
                let (d_ptr, d_ty) = self.emit_place_ptr(dest)?;
                let d_llvm = llvm_ty(self.tys(), d_ty)?;
                let was_closing_slot = if cancel.is_some() {
                    let pre_slot = self.fresh();
                    let pre_ct = self.fresh();
                    let pre_has = self.fresh();
                    let pre_chk = format!("cc_pre_{}", self.tmp);
                    let pre_done = format!("cc_predone_{}", self.tmp);
                    self.tmp += 1;
                    let pre_stp = self.fresh();
                    let pre_st = self.fresh();
                    let pre_is = self.fresh();
                    let _ = writeln!(self.out, "  {pre_slot} = alloca i1");
                    let _ = writeln!(self.out, "  store i1 0, ptr {pre_slot}");
                    let _ = writeln!(self.out, "  {pre_ct} = load ptr, ptr @__zil_current_task");
                    let _ = writeln!(self.out, "  {pre_has} = icmp ne ptr {pre_ct}, null");
                    let _ = writeln!(
                        self.out,
                        "  br i1 {pre_has}, label %{pre_chk}, label %{pre_done}"
                    );
                    let _ = writeln!(self.out, "{pre_chk}:");
                    let _ = writeln!(
                        self.out,
                        "  {pre_stp} = getelementptr inbounds %__zil_task_hdr, ptr {pre_ct}, i32 0, i32 0"
                    );
                    let _ = writeln!(self.out, "  {pre_st} = load i32, ptr {pre_stp}");
                    let _ = writeln!(self.out, "  {pre_is} = icmp eq i32 {pre_st}, 4");
                    let _ = writeln!(self.out, "  store i1 {pre_is}, ptr {pre_slot}");
                    let _ = writeln!(self.out, "  br label %{pre_done}");
                    let _ = writeln!(self.out, "{pre_done}:");
                    Some(pre_slot)
                } else {
                    None
                };
                match callee {
                    Callee::Direct(fid) => {
                        let mut arg_strs = Vec::new();
                        for a in args {
                            let (v, lt, _) = self.emit_operand(a)?;
                            if lt != "void" {
                                arg_strs.push(format!("{lt} {v}"));
                            }
                        }
                        let sym = fn_symbol(*fid, &self.module.functions[fid.index()]);
                        if d_llvm == "void" {
                            let _ =
                                writeln!(self.out, "  call void @{sym}({})", arg_strs.join(", "));
                        } else {
                            let r = self.fresh();
                            let _ = writeln!(
                                self.out,
                                "  {r} = call {d_llvm} @{sym}({})",
                                arg_strs.join(", ")
                            );
                            let _ = writeln!(self.out, "  store {d_llvm} {r}, ptr {d_ptr}");
                        }
                    }
                    Callee::Foreign(fid) => {
                        let mut arg_strs = Vec::new();
                        for a in args {
                            let (v, lt, _) = self.emit_operand(a)?;
                            if lt != "void" {
                                arg_strs.push(format!("{lt} {v}"));
                            }
                        }
                        let sym = sanitize_symbol(&self.module.foreigns[fid.index()].symbol);
                        if d_llvm == "void" {
                            let _ =
                                writeln!(self.out, "  call void @{sym}({})", arg_strs.join(", "));
                        } else {
                            let r = self.fresh();
                            let _ = writeln!(
                                self.out,
                                "  {r} = call {d_llvm} @{sym}({})",
                                arg_strs.join(", ")
                            );
                            let _ = writeln!(self.out, "  store {d_llvm} {r}, ptr {d_ptr}");
                        }
                    }
                    Callee::Indirect(op) => {
                        let (fptr, _, _) = self.emit_operand(op)?;
                        let mut arg_strs = Vec::new();
                        for a in args {
                            let (v, lt, _) = self.emit_operand(a)?;
                            if lt != "void" {
                                arg_strs.push(format!("{lt} {v}"));
                            }
                        }
                        if d_llvm == "void" {
                            let _ =
                                writeln!(self.out, "  call void {fptr}({})", arg_strs.join(", "));
                        } else {
                            let r = self.fresh();
                            let _ = writeln!(
                                self.out,
                                "  {r} = call {d_llvm} {fptr}({})",
                                arg_strs.join(", ")
                            );
                            let _ = writeln!(self.out, "  store {d_llvm} {r}, ptr {d_ptr}");
                        }
                    }
                    Callee::Closure {
                        closure,
                        mode: ClosureCallMode::Shared | ClosureCallMode::Mut | ClosureCallMode::Once,
                    } => {
                        let (env_ptr, c_ty) = self.emit_place_ptr(closure)?;
                        let Some(TyKind::Closure { def, .. }) = self.tys().get(c_ty) else {
                            return Err(BodyEmitError::new("closure call on non-closure"));
                        };
                        let c_fn_id = self
                            .module
                            .functions
                            .iter()
                            .position(|f| f.body.closure == Some(*def))
                            .map(FnId::from_index)
                            .ok_or_else(|| BodyEmitError::new("closure body function missing"))?;
                        let sym = fn_symbol(c_fn_id, &self.module.functions[c_fn_id.index()]);
                        let mut arg_strs = vec![format!("ptr {env_ptr}")];
                        for a in args {
                            let (v, lt, _) = self.emit_operand(a)?;
                            if lt != "void" {
                                arg_strs.push(format!("{lt} {v}"));
                            }
                        }
                        if d_llvm == "void" {
                            let _ =
                                writeln!(self.out, "  call void @{sym}({})", arg_strs.join(", "));
                        } else {
                            let r = self.fresh();
                            let _ = writeln!(
                                self.out,
                                "  {r} = call {d_llvm} @{sym}({})",
                                arg_strs.join(", ")
                            );
                            let _ = writeln!(self.out, "  store {d_llvm} {r}, ptr {d_ptr}");
                        }
                    }
                    Callee::Intrinsic(op) => self.emit_intrinsic(*op, args, &d_ptr, d_ty)?,
                    Callee::DynSlot { object, slot } => {
                        let (obj_v, obj_lt, _obj_ty) = self.emit_operand(object)?;
                        let fat = if obj_lt == "ptr" {
                            let f = self.fresh();
                            let _ = writeln!(self.out, "  {f} = load {{ ptr, ptr }}, ptr {obj_v}");
                            f
                        } else {
                            obj_v
                        };
                        let data = self.fresh();
                        let vt = self.fresh();
                        let slot_p = self.fresh();
                        let fn_p = self.fresh();
                        let _ =
                            writeln!(self.out, "  {data} = extractvalue {{ ptr, ptr }} {fat}, 0");
                        let _ = writeln!(self.out, "  {vt} = extractvalue {{ ptr, ptr }} {fat}, 1");
                        let slot_idx = (*slot as u64) + 1;
                        let _ = writeln!(
                            self.out,
                            "  {slot_p} = getelementptr inbounds ptr, ptr {vt}, i64 {slot_idx}"
                        );
                        let _ = writeln!(self.out, "  {fn_p} = load ptr, ptr {slot_p}");
                        let mut arg_strs = Vec::with_capacity(1 + args.len());
                        arg_strs.push(format!("ptr {data}"));
                        for a in args {
                            let (v, lt, _) = self.emit_operand(a)?;
                            if lt != "void" {
                                arg_strs.push(format!("{lt} {v}"));
                            }
                        }
                        if d_llvm == "void" {
                            let _ =
                                writeln!(self.out, "  call void {fn_p}({})", arg_strs.join(", "));
                        } else {
                            let r = self.fresh();
                            let _ = writeln!(
                                self.out,
                                "  {r} = call {d_llvm} {fn_p}({})",
                                arg_strs.join(", ")
                            );
                            let _ = writeln!(self.out, "  store {d_llvm} {r}, ptr {d_ptr}");
                        }
                    }
                }
                if let Some(cancel_bb) = cancel {
                    let was_cl = self.fresh();
                    let ct = self.fresh();
                    let has_ct = self.fresh();
                    let not_pre = self.fresh();
                    let can_chk = self.fresh();
                    let chk_lbl = format!("cc_chk_{}", self.tmp);
                    self.tmp += 1;
                    let cst_p = self.fresh();
                    let cst = self.fresh();
                    let is_canc = self.fresh();
                    let pre_slot = was_closing_slot.as_deref().unwrap();
                    let _ = writeln!(self.out, "  {was_cl} = load i1, ptr {pre_slot}");
                    let _ = writeln!(self.out, "  {not_pre} = xor i1 {was_cl}, true");
                    let _ = writeln!(self.out, "  {ct} = load ptr, ptr @__zil_current_task");
                    let _ = writeln!(self.out, "  {has_ct} = icmp ne ptr {ct}, null");
                    let _ = writeln!(self.out, "  {can_chk} = and i1 {not_pre}, {has_ct}");
                    let _ = writeln!(
                        self.out,
                        "  br i1 {can_chk}, label %{chk_lbl}, label %bb{}",
                        target.0
                    );
                    let _ = writeln!(self.out, "{chk_lbl}:");
                    let _ = writeln!(
                        self.out,
                        "  {cst_p} = getelementptr inbounds %__zil_task_hdr, ptr {ct}, i32 0, i32 0"
                    );
                    let _ = writeln!(self.out, "  {cst} = load i32, ptr {cst_p}");
                    let _ = writeln!(self.out, "  {is_canc} = icmp eq i32 {cst}, 4");
                    let _ = writeln!(
                        self.out,
                        "  br i1 {is_canc}, label %bb{}, label %bb{}",
                        cancel_bb.0, target.0
                    );
                } else {
                    let _ = writeln!(self.out, "  br label %bb{}", target.0);
                }
                Ok(())
            }
            Terminator::Assert {
                cond,
                expected,
                target,
                trap:
                    TrapKind::Bounds
                    | TrapKind::Overflow
                    | TrapKind::DivideByZero
                    | TrapKind::Conversion
                    | TrapKind::Explicit
                    | TrapKind::Cancel,
            } => {
                let (cv, _, _) = self.emit_operand(cond)?;
                let fail_bb = self.fresh();
                let fail_label = &fail_bb[1..];
                if *expected {
                    let _ = writeln!(
                        self.out,
                        "  br i1 {cv}, label %bb{}, label %{fail_label}",
                        target.0
                    );
                } else {
                    let _ = writeln!(
                        self.out,
                        "  br i1 {cv}, label %{fail_label}, label %bb{}",
                        target.0
                    );
                }
                let _ = writeln!(
                    self.out,
                    "{fail_label}:\n  call void @abort()\n  unreachable"
                );
                Ok(())
            }
            Terminator::Unreachable => {
                self.out.push_str("  unreachable\n");
                Ok(())
            }
            Terminator::Trap(TrapKind::Cancel) => {
                self.out
                    .push_str("  store i1 true, ptr @__zil_task_cancel_trap\n");
                let ret_llvm = llvm_ty(self.tys(), self.body.result)?;
                if ret_llvm == "void" {
                    self.out.push_str("  ret void\n");
                } else {
                    let _ = writeln!(self.out, "  ret {ret_llvm} zeroinitializer");
                }
                Ok(())
            }
            Terminator::Trap(
                TrapKind::Bounds
                | TrapKind::Overflow
                | TrapKind::DivideByZero
                | TrapKind::Conversion
                | TrapKind::Explicit,
            ) => {
                self.out.push_str("  call void @abort()\n  unreachable\n");
                Ok(())
            }
        }
    }

    fn emit_intrinsic(
        &mut self,
        op: IntrinsicOp,
        args: &[Operand],
        d_ptr: &str,
        d_ty: TyId,
    ) -> Result<(), BodyEmitError> {
        match op {
            IntrinsicOp::ManagedCreate => {
                // Initialize inline resource payload directly in `d_ptr` with zero heap allocation.
                let Some(TyKind::Managed(inner)) = self.tys().get(d_ty) else {
                    return Err(BodyEmitError::new("ManagedCreate dest is not Managed"));
                };
                let inner_llvm = llvm_storage_ty(self.tys(), *inner)?;
                if args.is_empty() {
                    let _ = writeln!(
                        self.out,
                        "  store {inner_llvm} zeroinitializer, ptr {d_ptr}"
                    );
                } else {
                    for (i, a) in args.iter().enumerate() {
                        let (v, lt, _) = self.emit_operand(a)?;
                        let gep = self.fresh();
                        let _ = writeln!(
                            self.out,
                            "  {gep} = getelementptr inbounds {inner_llvm}, ptr {d_ptr}, i32 0, i32 {i}"
                        );
                        if lt == "void" {
                            let _ = writeln!(self.out, "  store i8 0, ptr {gep}");
                        } else {
                            let _ = writeln!(self.out, "  store {lt} {v}, ptr {gep}");
                        }
                    }
                }
                Ok(())
            }
            IntrinsicOp::ByteBufferNew | IntrinsicOp::StoreNew => {
                // Inline header `{ ptr data, i64 len, i64 cap }`, zero-initialized without heap allocation.
                let _ = writeln!(
                    self.out,
                    "  store {{ ptr, i64, i64 }} zeroinitializer, ptr {d_ptr}"
                );
                Ok(())
            }
            IntrinsicOp::ByteBufferLen | IntrinsicOp::StoreLen => {
                let (h, _, _) = self.emit_operand(&args[0])?;
                let slot = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {slot} = getelementptr inbounds {{ ptr, i64, i64 }}, ptr {h}, i32 0, i32 1"
                );
                let val = self.fresh();
                let _ = writeln!(self.out, "  {val} = load i64, ptr {slot}");
                let _ = writeln!(self.out, "  store i64 {val}, ptr {d_ptr}");
                Ok(())
            }
            IntrinsicOp::ByteBufferCapacity | IntrinsicOp::StoreCapacity => {
                let (h, _, _) = self.emit_operand(&args[0])?;
                let slot = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {slot} = getelementptr inbounds {{ ptr, i64, i64 }}, ptr {h}, i32 0, i32 2"
                );
                let val = self.fresh();
                let _ = writeln!(self.out, "  {val} = load i64, ptr {slot}");
                let _ = writeln!(self.out, "  store i64 {val}, ptr {d_ptr}");
                Ok(())
            }
            IntrinsicOp::ByteBufferTruncate => {
                let (h, _, _) = self.emit_operand(&args[0])?;
                let (new_len, _, _) = self.emit_operand(&args[1])?;
                let len_slot = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {len_slot} = getelementptr inbounds {{ ptr, i64, i64 }}, ptr {h}, i32 0, i32 1"
                );
                let cur_len = self.fresh();
                let _ = writeln!(self.out, "  {cur_len} = load i64, ptr {len_slot}");
                let non_neg = self.fresh();
                let _ = writeln!(self.out, "  {non_neg} = icmp sge i64 {new_len}, 0");
                let le_cur = self.fresh();
                let _ = writeln!(self.out, "  {le_cur} = icmp sle i64 {new_len}, {cur_len}");
                let ok = self.fresh();
                let _ = writeln!(self.out, "  {ok} = and i1 {non_neg}, {le_cur}");
                let set_bb = self.fresh();
                let done_bb = self.fresh();
                let (sl, dl) = (&set_bb[1..], &done_bb[1..]);
                let _ = writeln!(self.out, "  br i1 {ok}, label %{sl}, label %{dl}");
                let _ = writeln!(self.out, "{sl}:");
                let _ = writeln!(self.out, "  store i64 {new_len}, ptr {len_slot}");
                let _ = writeln!(self.out, "  br label %{dl}");
                let _ = writeln!(self.out, "{dl}:");
                let _ = writeln!(self.out, "  store i1 {ok}, ptr {d_ptr}");
                Ok(())
            }
            IntrinsicOp::ByteBufferBorrow => {
                let (h, _, _) = self.emit_operand(&args[0])?;
                let data_slot = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {data_slot} = getelementptr inbounds {{ ptr, i64, i64 }}, ptr {h}, i32 0, i32 0"
                );
                let data = self.fresh();
                let _ = writeln!(self.out, "  {data} = load ptr, ptr {data_slot}");
                let len_slot = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {len_slot} = getelementptr inbounds {{ ptr, i64, i64 }}, ptr {h}, i32 0, i32 1"
                );
                let len = self.fresh();
                let _ = writeln!(self.out, "  {len} = load i64, ptr {len_slot}");
                let out_p0 = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {out_p0} = getelementptr inbounds {{ ptr, i64 }}, ptr {d_ptr}, i32 0, i32 0"
                );
                let _ = writeln!(self.out, "  store ptr {data}, ptr {out_p0}");
                let out_p1 = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {out_p1} = getelementptr inbounds {{ ptr, i64 }}, ptr {d_ptr}, i32 0, i32 1"
                );
                let _ = writeln!(self.out, "  store i64 {len}, ptr {out_p1}");
                Ok(())
            }
            IntrinsicOp::ByteBufferReserve | IntrinsicOp::StoreReserve => {
                let (h, _, h_ty) = self.emit_operand(&args[0])?;
                let (need, _, _) = self.emit_operand(&args[1])?;
                let elem_llvm = if op == IntrinsicOp::ByteBufferReserve {
                    "i8".to_string()
                } else {
                    let elem = self.store_elem_ty(h_ty)?;
                    llvm_storage_ty(self.tys(), elem)?
                };
                let data_slot = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {data_slot} = getelementptr inbounds {{ ptr, i64, i64 }}, ptr {h}, i32 0, i32 0"
                );
                let cap_slot = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {cap_slot} = getelementptr inbounds {{ ptr, i64, i64 }}, ptr {h}, i32 0, i32 2"
                );
                let cur_cap = self.fresh();
                let _ = writeln!(self.out, "  {cur_cap} = load i64, ptr {cap_slot}");
                let need_grow = self.fresh();
                let _ = writeln!(self.out, "  {need_grow} = icmp sgt i64 {need}, {cur_cap}");
                let grow_bb = self.fresh();
                let ok_bb = self.fresh();
                let done_bb = self.fresh();
                let (gl, ol, dl) = (&grow_bb[1..], &ok_bb[1..], &done_bb[1..]);
                let _ = writeln!(self.out, "  br i1 {need_grow}, label %{gl}, label %{ol}");
                let _ = writeln!(self.out, "{gl}:");
                let sz_ptr = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {sz_ptr} = getelementptr {elem_llvm}, ptr null, i32 1"
                );
                let elem_sz = self.fresh();
                let _ = writeln!(self.out, "  {elem_sz} = ptrtoint ptr {sz_ptr} to i64");
                let total_bytes = self.fresh();
                let _ = writeln!(self.out, "  {total_bytes} = mul i64 {need}, {elem_sz}");
                let old_data = self.fresh();
                let _ = writeln!(self.out, "  {old_data} = load ptr, ptr {data_slot}");
                let new_data = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {new_data} = call ptr @realloc(ptr {old_data}, i64 {total_bytes})"
                );
                let not_null = self.fresh();
                let _ = writeln!(self.out, "  {not_null} = icmp ne ptr {new_data}, null");
                let commit_bb = self.fresh();
                let fail_bb = self.fresh();
                let (cl, fl) = (&commit_bb[1..], &fail_bb[1..]);
                let _ = writeln!(self.out, "  br i1 {not_null}, label %{cl}, label %{fl}");
                let _ = writeln!(self.out, "{cl}:");
                let _ = writeln!(self.out, "  store ptr {new_data}, ptr {data_slot}");
                let _ = writeln!(self.out, "  store i64 {need}, ptr {cap_slot}");
                let _ = writeln!(self.out, "  br label %{ol}");
                let _ = writeln!(self.out, "{ol}:");
                let _ = writeln!(self.out, "  store i1 1, ptr {d_ptr}");
                let _ = writeln!(self.out, "  br label %{dl}");
                let _ = writeln!(self.out, "{fl}:");
                let _ = writeln!(self.out, "  store i1 0, ptr {d_ptr}");
                let _ = writeln!(self.out, "  br label %{dl}");
                let _ = writeln!(self.out, "{dl}:");
                Ok(())
            }
            IntrinsicOp::ByteBufferPush => {
                let (h, _, _) = self.emit_operand(&args[0])?;
                let (byte_v, _, _) = self.emit_operand(&args[1])?;
                let data_slot = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {data_slot} = getelementptr inbounds {{ ptr, i64, i64 }}, ptr {h}, i32 0, i32 0"
                );
                let len_slot = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {len_slot} = getelementptr inbounds {{ ptr, i64, i64 }}, ptr {h}, i32 0, i32 1"
                );
                let cap_slot = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {cap_slot} = getelementptr inbounds {{ ptr, i64, i64 }}, ptr {h}, i32 0, i32 2"
                );
                let len = self.fresh();
                let _ = writeln!(self.out, "  {len} = load i64, ptr {len_slot}");
                let cap = self.fresh();
                let _ = writeln!(self.out, "  {cap} = load i64, ptr {cap_slot}");
                let full = self.fresh();
                let _ = writeln!(self.out, "  {full} = icmp eq i64 {len}, {cap}");
                let grow_bb = self.fresh();
                let write_bb = self.fresh();
                let fail_bb = self.fresh();
                let done_bb = self.fresh();
                let (gl, wl, fl, dl) =
                    (&grow_bb[1..], &write_bb[1..], &fail_bb[1..], &done_bb[1..]);
                let _ = writeln!(self.out, "  br i1 {full}, label %{gl}, label %{wl}");
                let _ = writeln!(self.out, "{gl}:");
                let ok_alloc = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {ok_alloc} = call i1 @__zil_bytes_grow(ptr {h}, i64 {cap})"
                );
                let _ = writeln!(self.out, "  br i1 {ok_alloc}, label %{wl}, label %{fl}");
                let _ = writeln!(self.out, "{wl}:");
                let data = self.fresh();
                let _ = writeln!(self.out, "  {data} = load ptr, ptr {data_slot}");
                let dst = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {dst} = getelementptr inbounds i8, ptr {data}, i64 {len}"
                );
                let _ = writeln!(self.out, "  store i8 {byte_v}, ptr {dst}");
                let next_len = self.fresh();
                let _ = writeln!(self.out, "  {next_len} = add i64 {len}, 1");
                let _ = writeln!(self.out, "  store i64 {next_len}, ptr {len_slot}");
                let _ = writeln!(self.out, "  store i1 1, ptr {d_ptr}");
                let _ = writeln!(self.out, "  br label %{dl}");
                let _ = writeln!(self.out, "{fl}:");
                let _ = writeln!(self.out, "  store i1 0, ptr {d_ptr}");
                let _ = writeln!(self.out, "  br label %{dl}");
                let _ = writeln!(self.out, "{dl}:");
                Ok(())
            }
            IntrinsicOp::StorePush => {
                let (h, _, h_ty) = self.emit_operand(&args[0])?;
                let (val, val_lt, _) = self.emit_operand(&args[1])?;
                let elem = self.store_elem_ty(h_ty)?;
                let elem_llvm = llvm_storage_ty(self.tys(), elem)?;
                let d_llvm = llvm_storage_ty(self.tys(), d_ty)?;
                let data_slot = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {data_slot} = getelementptr inbounds {{ ptr, i64, i64 }}, ptr {h}, i32 0, i32 0"
                );
                let len_slot = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {len_slot} = getelementptr inbounds {{ ptr, i64, i64 }}, ptr {h}, i32 0, i32 1"
                );
                let cap_slot = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {cap_slot} = getelementptr inbounds {{ ptr, i64, i64 }}, ptr {h}, i32 0, i32 2"
                );
                let len = self.fresh();
                let _ = writeln!(self.out, "  {len} = load i64, ptr {len_slot}");
                let cap = self.fresh();
                let _ = writeln!(self.out, "  {cap} = load i64, ptr {cap_slot}");
                let has_space = self.fresh();
                let _ = writeln!(self.out, "  {has_space} = icmp slt i64 {len}, {cap}");
                let write_bb = self.fresh();
                let fail_bb = self.fresh();
                let done_bb = self.fresh();
                let (wl, fl, dl) = (&write_bb[1..], &fail_bb[1..], &done_bb[1..]);
                let _ = writeln!(self.out, "  br i1 {has_space}, label %{wl}, label %{fl}");
                let _ = writeln!(self.out, "{wl}:");
                let data = self.fresh();
                let _ = writeln!(self.out, "  {data} = load ptr, ptr {data_slot}");
                let dst = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {dst} = getelementptr inbounds {elem_llvm}, ptr {data}, i64 {len}"
                );
                if val_lt == "void" {
                    let _ = writeln!(self.out, "  store i8 0, ptr {dst}");
                } else {
                    let _ = writeln!(self.out, "  store {val_lt} {val}, ptr {dst}");
                }
                let next_len = self.fresh();
                let _ = writeln!(self.out, "  {next_len} = add i64 {len}, 1");
                let _ = writeln!(self.out, "  store i64 {next_len}, ptr {len_slot}");
                let ok_tag = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {ok_tag} = getelementptr inbounds {d_llvm}, ptr {d_ptr}, i32 0, i32 0"
                );
                let _ = writeln!(self.out, "  store i1 1, ptr {ok_tag}");
                let _ = writeln!(self.out, "  br label %{dl}");
                let _ = writeln!(self.out, "{fl}:");
                let err_tag = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {err_tag} = getelementptr inbounds {d_llvm}, ptr {d_ptr}, i32 0, i32 0"
                );
                let _ = writeln!(self.out, "  store i1 0, ptr {err_tag}");
                let err_val = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {err_val} = getelementptr inbounds {d_llvm}, ptr {d_ptr}, i32 0, i32 2"
                );
                if val_lt == "void" {
                    let _ = writeln!(self.out, "  store i8 0, ptr {err_val}");
                } else {
                    let _ = writeln!(self.out, "  store {val_lt} {val}, ptr {err_val}");
                }
                let _ = writeln!(self.out, "  br label %{dl}");
                let _ = writeln!(self.out, "{dl}:");
                Ok(())
            }
            IntrinsicOp::StorePop => {
                let (h, _, h_ty) = self.emit_operand(&args[0])?;
                let elem = self.store_elem_ty(h_ty)?;
                let elem_llvm = llvm_storage_ty(self.tys(), elem)?;
                let d_llvm = llvm_storage_ty(self.tys(), d_ty)?;
                let data_slot = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {data_slot} = getelementptr inbounds {{ ptr, i64, i64 }}, ptr {h}, i32 0, i32 0"
                );
                let len_slot = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {len_slot} = getelementptr inbounds {{ ptr, i64, i64 }}, ptr {h}, i32 0, i32 1"
                );
                let len = self.fresh();
                let _ = writeln!(self.out, "  {len} = load i64, ptr {len_slot}");
                let has = self.fresh();
                let _ = writeln!(self.out, "  {has} = icmp sgt i64 {len}, 0");
                let ok_bb = self.fresh();
                let empty_bb = self.fresh();
                let done_bb = self.fresh();
                let (ol, el, dl) = (&ok_bb[1..], &empty_bb[1..], &done_bb[1..]);
                let _ = writeln!(self.out, "  br i1 {has}, label %{ol}, label %{el}");
                let _ = writeln!(self.out, "{ol}:");
                let next_len = self.fresh();
                let _ = writeln!(self.out, "  {next_len} = sub i64 {len}, 1");
                let _ = writeln!(self.out, "  store i64 {next_len}, ptr {len_slot}");
                let data = self.fresh();
                let _ = writeln!(self.out, "  {data} = load ptr, ptr {data_slot}");
                let src = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {src} = getelementptr inbounds {elem_llvm}, ptr {data}, i64 {next_len}"
                );
                let loaded = self.fresh();
                let _ = writeln!(self.out, "  {loaded} = load {elem_llvm}, ptr {src}");
                let ok_tag = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {ok_tag} = getelementptr inbounds {d_llvm}, ptr {d_ptr}, i32 0, i32 0"
                );
                let _ = writeln!(self.out, "  store i1 1, ptr {ok_tag}");
                let ok_val = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {ok_val} = getelementptr inbounds {d_llvm}, ptr {d_ptr}, i32 0, i32 1"
                );
                let _ = writeln!(self.out, "  store {elem_llvm} {loaded}, ptr {ok_val}");
                let _ = writeln!(self.out, "  br label %{dl}");
                let _ = writeln!(self.out, "{el}:");
                let err_tag = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {err_tag} = getelementptr inbounds {d_llvm}, ptr {d_ptr}, i32 0, i32 0"
                );
                let _ = writeln!(self.out, "  store i1 0, ptr {err_tag}");
                let _ = writeln!(self.out, "  br label %{dl}");
                let _ = writeln!(self.out, "{dl}:");
                Ok(())
            }
            IntrinsicOp::StoreBorrow { .. } => {
                let (h, _, h_ty) = self.emit_operand(&args[0])?;
                let (idx, _, _) = self.emit_operand(&args[1])?;
                let elem = self.store_elem_ty(h_ty)?;
                let elem_llvm = llvm_storage_ty(self.tys(), elem)?;
                let data_slot = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {data_slot} = getelementptr inbounds {{ ptr, i64, i64 }}, ptr {h}, i32 0, i32 0"
                );
                let len_slot = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {len_slot} = getelementptr inbounds {{ ptr, i64, i64 }}, ptr {h}, i32 0, i32 1"
                );
                let len = self.fresh();
                let _ = writeln!(self.out, "  {len} = load i64, ptr {len_slot}");
                let non_neg = self.fresh();
                let _ = writeln!(self.out, "  {non_neg} = icmp sge i64 {idx}, 0");
                let in_len = self.fresh();
                let _ = writeln!(self.out, "  {in_len} = icmp slt i64 {idx}, {len}");
                let in_bounds = self.fresh();
                let _ = writeln!(self.out, "  {in_bounds} = and i1 {non_neg}, {in_len}");
                let ok_bb = self.fresh();
                let oob_bb = self.fresh();
                let (ol, fl) = (&ok_bb[1..], &oob_bb[1..]);
                let _ = writeln!(self.out, "  br i1 {in_bounds}, label %{ol}, label %{fl}");
                let _ = writeln!(self.out, "{fl}:\n  call void @abort()\n  unreachable");
                let _ = writeln!(self.out, "{ol}:");
                let data = self.fresh();
                let _ = writeln!(self.out, "  {data} = load ptr, ptr {data_slot}");
                let gep = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {gep} = getelementptr inbounds {elem_llvm}, ptr {data}, i64 {idx}"
                );
                let _ = writeln!(self.out, "  store ptr {gep}, ptr {d_ptr}");
                Ok(())
            }
            IntrinsicOp::MutexCreate => {
                let (val, val_lt, _) = self.emit_operand(&args[0])?;
                let d_llvm = llvm_storage_ty(self.tys(), d_ty)?;
                let store_lt = if val_lt == "void" { "i8" } else { &val_lt };
                let box_ty = format!("{{ [8 x i64], {store_lt} }}");
                let tid = self.tmp;
                self.tmp += 1;
                let sz_ptr = self.fresh();
                let sz = self.fresh();
                let raw = self.fresh();
                let is_null = self.fresh();
                let l_alloc_ok = format!("mc_aok_{tid}");
                let l_init_ok = format!("mc_iok_{tid}");
                let l_init_fail = format!("mc_ifail_{tid}");
                let l_fail = format!("mc_fail_{tid}");
                let l_done = format!("mc_done_{tid}");
                let _ = writeln!(
                    self.out,
                    "  {sz_ptr} = getelementptr {box_ty}, ptr null, i32 1"
                );
                let _ = writeln!(self.out, "  {sz} = ptrtoint ptr {sz_ptr} to i64");
                let _ = writeln!(self.out, "  {raw} = call ptr @calloc(i64 1, i64 {sz})");
                let _ = writeln!(self.out, "  {is_null} = icmp eq ptr {raw}, null");
                let _ = writeln!(
                    self.out,
                    "  br i1 {is_null}, label %{l_fail}, label %{l_alloc_ok}"
                );
                let _ = writeln!(self.out, "{l_alloc_ok}:");
                if self.is_windows {
                    let _ = writeln!(self.out, "  call void @InitializeSRWLock(ptr {raw})");
                    let _ = writeln!(self.out, "  br label %{l_init_ok}");
                } else {
                    let init_rc = self.fresh();
                    let init_ok = self.fresh();
                    let _ = writeln!(
                        self.out,
                        "  {init_rc} = call i32 @pthread_mutex_init(ptr {raw}, ptr null)"
                    );
                    let _ = writeln!(self.out, "  {init_ok} = icmp eq i32 {init_rc}, 0");
                    let _ = writeln!(
                        self.out,
                        "  br i1 {init_ok}, label %{l_init_ok}, label %{l_init_fail}"
                    );
                    let _ = writeln!(self.out, "{l_init_fail}:");
                    let _ = writeln!(self.out, "  call void @free(ptr {raw})");
                    let _ = writeln!(self.out, "  br label %{l_fail}");
                }
                let _ = writeln!(self.out, "{l_fail}:");
                let e_tag = self.fresh();
                let e_slot = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {e_tag} = getelementptr inbounds {d_llvm}, ptr {d_ptr}, i32 0, i32 0"
                );
                let _ = writeln!(self.out, "  store i1 0, ptr {e_tag}");
                let _ = writeln!(
                    self.out,
                    "  {e_slot} = getelementptr inbounds {d_llvm}, ptr {d_ptr}, i32 0, i32 2"
                );
                let _ = writeln!(self.out, "  store i32 0, ptr {e_slot}");
                let _ = writeln!(self.out, "  br label %{l_done}");
                let _ = writeln!(self.out, "{l_init_ok}:");
                let pay_p = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {pay_p} = getelementptr inbounds {box_ty}, ptr {raw}, i32 0, i32 1"
                );
                if val_lt == "void" {
                    let _ = writeln!(self.out, "  store i8 0, ptr {pay_p}");
                } else {
                    let _ = writeln!(self.out, "  store {val_lt} {val}, ptr {pay_p}");
                }
                let ok_tag = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {ok_tag} = getelementptr inbounds {d_llvm}, ptr {d_ptr}, i32 0, i32 0"
                );
                let _ = writeln!(self.out, "  store i1 1, ptr {ok_tag}");
                let ok_val = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {ok_val} = getelementptr inbounds {d_llvm}, ptr {d_ptr}, i32 0, i32 1"
                );
                let _ = writeln!(self.out, "  store ptr {raw}, ptr {ok_val}");
                let _ = writeln!(self.out, "  br label %{l_done}");
                let _ = writeln!(self.out, "{l_done}:");
                Ok(())
            }
            IntrinsicOp::MutexLock => {
                let (h, _, _) = self.emit_operand(&args[0])?;
                if self.is_windows {
                    let _ = writeln!(self.out, "  call void @AcquireSRWLockExclusive(ptr {h})");
                } else {
                    let tid = self.tmp;
                    self.tmp += 1;
                    let rc = self.fresh();
                    let ok = self.fresh();
                    let l_ok = format!("ml_ok_{tid}");
                    let l_ab = format!("ml_ab_{tid}");
                    let _ = writeln!(self.out, "  {rc} = call i32 @pthread_mutex_lock(ptr {h})");
                    let _ = writeln!(self.out, "  {ok} = icmp eq i32 {rc}, 0");
                    let _ = writeln!(self.out, "  br i1 {ok}, label %{l_ok}, label %{l_ab}");
                    let _ = writeln!(self.out, "{l_ab}:");
                    let _ = writeln!(self.out, "  call void @abort()");
                    let _ = writeln!(self.out, "  unreachable");
                    let _ = writeln!(self.out, "{l_ok}:");
                }
                let _ = writeln!(self.out, "  store ptr {h}, ptr {d_ptr}");
                Ok(())
            }
            IntrinsicOp::MutexValue => {
                let (guard_ptr, _, _) = self.emit_operand(&args[0])?;
                let handle = self.fresh();
                let pay = self.fresh();
                let _ = writeln!(self.out, "  {handle} = load ptr, ptr {guard_ptr}");
                let _ = writeln!(
                    self.out,
                    "  {pay} = getelementptr inbounds i8, ptr {handle}, i64 64"
                );
                let _ = writeln!(self.out, "  store ptr {pay}, ptr {d_ptr}");
                Ok(())
            }
            IntrinsicOp::MutexUnlock => {
                let (h, _, _) = self.emit_operand(&args[0])?;
                if self.is_windows {
                    let _ = writeln!(self.out, "  call void @ReleaseSRWLockExclusive(ptr {h})");
                } else {
                    let tid = self.tmp;
                    self.tmp += 1;
                    let rc = self.fresh();
                    let ok = self.fresh();
                    let l_ok = format!("mu_ok_{tid}");
                    let l_ab = format!("mu_ab_{tid}");
                    let _ = writeln!(self.out, "  {rc} = call i32 @pthread_mutex_unlock(ptr {h})");
                    let _ = writeln!(self.out, "  {ok} = icmp eq i32 {rc}, 0");
                    let _ = writeln!(self.out, "  br i1 {ok}, label %{l_ok}, label %{l_ab}");
                    let _ = writeln!(self.out, "{l_ab}:");
                    let _ = writeln!(self.out, "  call void @abort()");
                    let _ = writeln!(self.out, "  unreachable");
                    let _ = writeln!(self.out, "{l_ok}:");
                }
                Ok(())
            }
            IntrinsicOp::SyncInit => {
                let (val, lt, _) = self.emit_operand(&args[0])?;
                let _ = writeln!(
                    self.out,
                    "  store atomic {lt} {val}, ptr {d_ptr} seq_cst, align 8"
                );
                Ok(())
            }
            IntrinsicOp::SyncLoad | IntrinsicOp::SyncLoadOrdered(_) => {
                let ord = if let IntrinsicOp::SyncLoadOrdered(o) = op {
                    o.llvm_name()
                } else {
                    "seq_cst"
                };
                let (cell_ptr, _, _) = self.emit_operand(&args[0])?;
                let lt = llvm_ty(self.tys(), d_ty)?;
                let val = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {val} = load atomic {lt}, ptr {cell_ptr} {ord}, align 8"
                );
                let _ = writeln!(self.out, "  store {lt} {val}, ptr {d_ptr}");
                Ok(())
            }
            IntrinsicOp::SyncStore | IntrinsicOp::SyncStoreOrdered(_) => {
                let ord = if let IntrinsicOp::SyncStoreOrdered(o) = op {
                    o.llvm_name()
                } else {
                    "seq_cst"
                };
                let (cell_ptr, _, _) = self.emit_operand(&args[0])?;
                let (val, lt, _) = self.emit_operand(&args[1])?;
                let _ = writeln!(
                    self.out,
                    "  store atomic {lt} {val}, ptr {cell_ptr} {ord}, align 8"
                );
                Ok(())
            }
            IntrinsicOp::SyncFetchAdd | IntrinsicOp::SyncFetchAddOrdered(_) => {
                let ord = if let IntrinsicOp::SyncFetchAddOrdered(o) = op {
                    o.llvm_name()
                } else {
                    "seq_cst"
                };
                let (cell_ptr, _, _) = self.emit_operand(&args[0])?;
                let (delta, lt, _) = self.emit_operand(&args[1])?;
                let old = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {old} = atomicrmw add ptr {cell_ptr}, {lt} {delta} {ord}"
                );
                let _ = writeln!(self.out, "  store {lt} {old}, ptr {d_ptr}");
                Ok(())
            }
            IntrinsicOp::SyncExchangeOrdered(o) => {
                let ord = o.llvm_name();
                let (cell_ptr, _, _) = self.emit_operand(&args[0])?;
                let (val, lt, _) = self.emit_operand(&args[1])?;
                let old = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {old} = atomicrmw xchg ptr {cell_ptr}, {lt} {val} {ord}"
                );
                let _ = writeln!(self.out, "  store {lt} {old}, ptr {d_ptr}");
                Ok(())
            }
            IntrinsicOp::SyncCompareExchange | IntrinsicOp::SyncCompareExchangeOrdered { .. } => {
                let (succ_ord, fail_ord) =
                    if let IntrinsicOp::SyncCompareExchangeOrdered { success, failure } = op {
                        (success.llvm_name(), failure.llvm_name())
                    } else {
                        ("seq_cst", "seq_cst")
                    };
                let (cell_ptr, _, _) = self.emit_operand(&args[0])?;
                let (expected, lt, _) = self.emit_operand(&args[1])?;
                let (desired, _, _) = self.emit_operand(&args[2])?;
                let pair = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {pair} = cmpxchg ptr {cell_ptr}, {lt} {expected}, {lt} {desired} {succ_ord} {fail_ord}"
                );
                let obs = self.fresh();
                let ok = self.fresh();
                let p0 = self.fresh();
                let p1 = self.fresh();
                let _ = writeln!(self.out, "  {obs} = extractvalue {{ {lt}, i1 }} {pair}, 0");
                let _ = writeln!(self.out, "  {ok} = extractvalue {{ {lt}, i1 }} {pair}, 1");
                let _ = writeln!(
                    self.out,
                    "  {p0} = getelementptr inbounds {{ {lt}, i1 }}, ptr {d_ptr}, i32 0, i32 0"
                );
                let _ = writeln!(self.out, "  store {lt} {obs}, ptr {p0}");
                let _ = writeln!(
                    self.out,
                    "  {p1} = getelementptr inbounds {{ {lt}, i1 }}, ptr {d_ptr}, i32 0, i32 1"
                );
                let _ = writeln!(self.out, "  store i1 {ok}, ptr {p1}");
                Ok(())
            }
            IntrinsicOp::CNull => {
                let _ = writeln!(self.out, "  store ptr null, ptr {d_ptr}");
                Ok(())
            }
            IntrinsicOp::CCallback => {
                let (fn_ptr, _, _) = self.emit_operand(&args[0])?;
                let _ = writeln!(self.out, "  store ptr {fn_ptr}, ptr {d_ptr}");
                Ok(())
            }
            IntrinsicOp::ThreadScope => {
                // Real OS-thread execution of scoped worker(arg) concurrently with parent().
                let (worker_fn, _, _) = self.emit_operand(&args[0])?;
                let (cell_v, cell_lt, _) = self.emit_operand(&args[1])?;
                let (is_enum_dest, r1_ty_id, r2_ty_id) = match self.tys().get(d_ty).cloned() {
                    Some(TyKind::Tuple(ret_elems)) if ret_elems.len() == 3 => {
                        (false, ret_elems[1], ret_elems[2])
                    }
                    Some(TyKind::Adt {
                        adt,
                        args: dest_args,
                    }) => {
                        let adef = self
                            .tys()
                            .get_adt(adt)
                            .ok_or_else(|| BodyEmitError::new("ThreadScope missing result ADT"))?;
                        if adef.kind != AdtKind::Enum
                            || adef.variants.len() != 2
                            || adef.variants[1].fields.len() != 2
                        {
                            return Err(BodyEmitError::new(
                                "ThreadScope enum dest must be TaskThreadScopeResult",
                            ));
                        }
                        let r1_id = self
                            .tys()
                            .subst_lookup(adef.variants[1].fields[0].ty, dest_args)
                            .ok_or_else(|| BodyEmitError::new("ThreadScope missing R1 type"))?;
                        let r2_id = self
                            .tys()
                            .subst_lookup(adef.variants[1].fields[1].ty, dest_args)
                            .ok_or_else(|| BodyEmitError::new("ThreadScope missing R2 type"))?;
                        (true, r1_id, r2_id)
                    }
                    _ => {
                        return Err(BodyEmitError::new(
                            "ThreadScope dest must be 3-tuple or TaskThreadScopeResult",
                        ));
                    }
                };
                let r1_ty = llvm_ty(self.tys(), r1_ty_id)?;
                let r1_stor = llvm_storage_ty(self.tys(), r1_ty_id)?;
                let r2_ty = llvm_ty(self.tys(), r2_ty_id)?;
                let d_llvm = llvm_storage_ty(self.tys(), d_ty)?;
                let (env_ptr, env_ty) = match &args[2] {
                    Operand::Copy(p) | Operand::Move(p) => self.emit_place_ptr(p)?,
                    Operand::Const(_) | Operand::Pooled(_) => {
                        return Err(BodyEmitError::new("ThreadScope parent must be a closure"));
                    }
                };
                let Some(TyKind::Closure { def, .. }) = self.tys().get(env_ty) else {
                    return Err(BodyEmitError::new("ThreadScope parent is not a closure"));
                };
                let c_fn_id = self
                    .module
                    .functions
                    .iter()
                    .position(|f| f.body.closure == Some(*def))
                    .map(FnId::from_index)
                    .ok_or_else(|| BodyEmitError::new("ThreadScope closure body missing"))?;
                let sym = fn_symbol(c_fn_id, &self.module.functions[c_fn_id.index()]);

                let cell_stor = if cell_lt == "void" { "i8" } else { &cell_lt };
                let ctx_ty = format!("{{ ptr, {cell_stor}, {r1_stor} }}");
                let tid = self.tmp;
                self.tmp += 1;
                let entry_sym = format!("__zil_thread_entry_f{}_{tid}", self.fn_id.0);

                // Emit worker trampoline into extra_defs.
                if self.is_windows {
                    let _ = writeln!(
                        self.extra_defs,
                        "\ndefine internal i32 @{entry_sym}(ptr %arg) {{"
                    );
                } else {
                    let _ = writeln!(
                        self.extra_defs,
                        "\ndefine internal ptr @{entry_sym}(ptr %arg) {{"
                    );
                }
                self.extra_defs.push_str("entry:\n");
                let _ = writeln!(
                    self.extra_defs,
                    "  %fn_p = getelementptr inbounds {ctx_ty}, ptr %arg, i32 0, i32 0"
                );
                self.extra_defs.push_str("  %wfn = load ptr, ptr %fn_p\n");
                let call_arg = if cell_lt == "void" {
                    String::new()
                } else {
                    let _ = writeln!(
                        self.extra_defs,
                        "  %ap = getelementptr inbounds {ctx_ty}, ptr %arg, i32 0, i32 1"
                    );
                    let _ = writeln!(self.extra_defs, "  %av = load {cell_lt}, ptr %ap");
                    format!("{cell_lt} %av")
                };
                if r1_ty == "void" {
                    let _ = writeln!(self.extra_defs, "  call void %wfn({call_arg})");
                } else {
                    let _ = writeln!(self.extra_defs, "  %rv = call {r1_ty} %wfn({call_arg})");
                    let _ = writeln!(
                        self.extra_defs,
                        "  %rp = getelementptr inbounds {ctx_ty}, ptr %arg, i32 0, i32 2"
                    );
                    let _ = writeln!(self.extra_defs, "  store {r1_ty} %rv, ptr %rp");
                }
                if self.is_windows {
                    self.extra_defs.push_str("  ret i32 0\n}\n");
                } else {
                    self.extra_defs.push_str("  ret ptr null\n}\n");
                }

                let ctx = self.fresh();
                let c_fn_p = self.fresh();
                let pc_ok = self.fresh();
                let l_ok = format!("ts_ok_{tid}");
                let l_fail = format!("ts_fail_{tid}");
                let l_joined = format!("ts_join_{tid}");
                let l_abort = format!("ts_ab_{tid}");
                let l_done = format!("ts_done_{tid}");

                let _ = writeln!(self.out, "  {ctx} = alloca {ctx_ty}");
                let (tid_slot, win_thread_handle) = if self.is_windows {
                    (String::new(), self.fresh())
                } else {
                    let slot = self.fresh();
                    let _ = writeln!(self.out, "  {slot} = alloca i64");
                    (slot, String::new())
                };
                let _ = writeln!(
                    self.out,
                    "  {c_fn_p} = getelementptr inbounds {ctx_ty}, ptr {ctx}, i32 0, i32 0"
                );
                let _ = writeln!(self.out, "  store ptr {worker_fn}, ptr {c_fn_p}");
                if cell_lt != "void" {
                    let c_arg_p = self.fresh();
                    let _ = writeln!(
                        self.out,
                        "  {c_arg_p} = getelementptr inbounds {ctx_ty}, ptr {ctx}, i32 0, i32 1"
                    );
                    let _ = writeln!(self.out, "  store {cell_lt} {cell_v}, ptr {c_arg_p}");
                }
                if self.is_windows {
                    let _ = writeln!(
                        self.out,
                        "  {win_thread_handle} = call ptr @_beginthreadex(ptr null, i32 0, ptr @{entry_sym}, ptr {ctx}, i32 0, ptr null)"
                    );
                    let _ = writeln!(
                        self.out,
                        "  {pc_ok} = icmp ne ptr {win_thread_handle}, null"
                    );
                } else {
                    let pc_rc = self.fresh();
                    let _ = writeln!(
                        self.out,
                        "  {pc_rc} = call i32 @pthread_create(ptr {tid_slot}, ptr null, ptr @{entry_sym}, ptr {ctx})"
                    );
                    let _ = writeln!(self.out, "  {pc_ok} = icmp eq i32 {pc_rc}, 0");
                }
                let _ = writeln!(self.out, "  br i1 {pc_ok}, label %{l_ok}, label %{l_fail}");

                // Thread creation failed:
                // - Tuple dest: (false, zero, zero)
                // - TaskThreadScopeResult enum dest: Failed(cell_v) (tag 0, returning unconsumed Task)
                let _ = writeln!(self.out, "{l_fail}:");
                let _ = writeln!(self.out, "  store {d_llvm} zeroinitializer, ptr {d_ptr}");
                if is_enum_dest && cell_lt != "void" {
                    let tag_p = self.fresh();
                    let _ = writeln!(
                        self.out,
                        "  {tag_p} = getelementptr inbounds {d_llvm}, ptr {d_ptr}, i32 0, i32 0"
                    );
                    let _ = writeln!(self.out, "  store i32 0, ptr {tag_p}");
                    let fail_p = self.fresh();
                    let _ = writeln!(
                        self.out,
                        "  {fail_p} = getelementptr inbounds {d_llvm}, ptr {d_ptr}, i32 0, i32 1, i32 0"
                    );
                    let _ = writeln!(self.out, "  store {cell_lt} {cell_v}, ptr {fail_p}");
                }
                let _ = writeln!(self.out, "  br label %{l_done}");

                // Thread created -> run parent concurrently, then join.
                let _ = writeln!(self.out, "{l_ok}:");
                let r2 = if r2_ty == "void" {
                    let _ = writeln!(self.out, "  call void @{sym}(ptr {env_ptr})");
                    "0".to_string()
                } else {
                    let r = self.fresh();
                    let _ = writeln!(self.out, "  {r} = call {r2_ty} @{sym}(ptr {env_ptr})");
                    r
                };
                if self.is_windows {
                    let wrc = self.fresh();
                    let crc = self.fresh();
                    let w_ok = self.fresh();
                    let c_ok = self.fresh();
                    let pj_ok = self.fresh();
                    let _ = writeln!(
                        self.out,
                        "  {wrc} = call i32 @WaitForSingleObject(ptr {win_thread_handle}, i32 -1)"
                    );
                    let _ = writeln!(
                        self.out,
                        "  {crc} = call i32 @CloseHandle(ptr {win_thread_handle})"
                    );
                    let _ = writeln!(self.out, "  {w_ok} = icmp eq i32 {wrc}, 0");
                    let _ = writeln!(self.out, "  {c_ok} = icmp ne i32 {crc}, 0");
                    let _ = writeln!(self.out, "  {pj_ok} = and i1 {w_ok}, {c_ok}");
                    let _ = writeln!(
                        self.out,
                        "  br i1 {pj_ok}, label %{l_joined}, label %{l_abort}"
                    );
                } else {
                    let tid_val = self.fresh();
                    let pj_rc = self.fresh();
                    let pj_ok = self.fresh();
                    let _ = writeln!(self.out, "  {tid_val} = load i64, ptr {tid_slot}");
                    let _ = writeln!(
                        self.out,
                        "  {pj_rc} = call i32 @pthread_join(i64 {tid_val}, ptr null)"
                    );
                    let _ = writeln!(self.out, "  {pj_ok} = icmp eq i32 {pj_rc}, 0");
                    let _ = writeln!(
                        self.out,
                        "  br i1 {pj_ok}, label %{l_joined}, label %{l_abort}"
                    );
                }
                let _ = writeln!(self.out, "{l_abort}:");
                let _ = writeln!(self.out, "  call void @abort()");
                let _ = writeln!(self.out, "  unreachable");

                let _ = writeln!(self.out, "{l_joined}:");
                if is_enum_dest {
                    let tag_p = self.fresh();
                    let _ = writeln!(
                        self.out,
                        "  {tag_p} = getelementptr inbounds {d_llvm}, ptr {d_ptr}, i32 0, i32 0"
                    );
                    let _ = writeln!(self.out, "  store i32 1, ptr {tag_p}");
                    let f1 = self.fresh();
                    let _ = writeln!(
                        self.out,
                        "  {f1} = getelementptr inbounds {d_llvm}, ptr {d_ptr}, i32 0, i32 2, i32 0"
                    );
                    if r1_ty == "void" {
                        let _ = writeln!(self.out, "  store i8 0, ptr {f1}");
                    } else {
                        let r1_p = self.fresh();
                        let r1 = self.fresh();
                        let _ = writeln!(
                            self.out,
                            "  {r1_p} = getelementptr inbounds {ctx_ty}, ptr {ctx}, i32 0, i32 2"
                        );
                        let _ = writeln!(self.out, "  {r1} = load {r1_ty}, ptr {r1_p}");
                        let _ = writeln!(self.out, "  store {r1_ty} {r1}, ptr {f1}");
                    }
                    let f2 = self.fresh();
                    let _ = writeln!(
                        self.out,
                        "  {f2} = getelementptr inbounds {d_llvm}, ptr {d_ptr}, i32 0, i32 2, i32 1"
                    );
                    if r2_ty == "void" {
                        let _ = writeln!(self.out, "  store i8 0, ptr {f2}");
                    } else {
                        let _ = writeln!(self.out, "  store {r2_ty} {r2}, ptr {f2}");
                    }
                } else {
                    let f0 = self.fresh();
                    let _ = writeln!(
                        self.out,
                        "  {f0} = getelementptr inbounds {d_llvm}, ptr {d_ptr}, i32 0, i32 0"
                    );
                    let _ = writeln!(self.out, "  store i1 1, ptr {f0}");
                    let f1 = self.fresh();
                    let _ = writeln!(
                        self.out,
                        "  {f1} = getelementptr inbounds {d_llvm}, ptr {d_ptr}, i32 0, i32 1"
                    );
                    if r1_ty == "void" {
                        let _ = writeln!(self.out, "  store i8 0, ptr {f1}");
                    } else {
                        let r1_p = self.fresh();
                        let r1 = self.fresh();
                        let _ = writeln!(
                            self.out,
                            "  {r1_p} = getelementptr inbounds {ctx_ty}, ptr {ctx}, i32 0, i32 2"
                        );
                        let _ = writeln!(self.out, "  {r1} = load {r1_ty}, ptr {r1_p}");
                        let _ = writeln!(self.out, "  store {r1_ty} {r1}, ptr {f1}");
                    }
                    let f2 = self.fresh();
                    let _ = writeln!(
                        self.out,
                        "  {f2} = getelementptr inbounds {d_llvm}, ptr {d_ptr}, i32 0, i32 2"
                    );
                    if r2_ty == "void" {
                        let _ = writeln!(self.out, "  store i8 0, ptr {f2}");
                    } else {
                        let _ = writeln!(self.out, "  store {r2_ty} {r2}, ptr {f2}");
                    }
                }
                let _ = writeln!(self.out, "  br label %{l_done}");
                let _ = writeln!(self.out, "{l_done}:");
                Ok(())
            }
            IntrinsicOp::TaskYield => {
                let tid = self.tmp;
                self.tmp += 1;
                let f = self.fresh();
                let f_null = self.fresh();
                let st = self.fresh();
                let cur_st = self.fresh();
                let is_cl = self.fresh();
                let next_st = self.fresh();
                let cctx = self.fresh();
                let l_have = format!("ty_have_{tid}");
                let l_ok = format!("ty_ok_{tid}");
                let l_ab = format!("ty_ab_{tid}");
                let _ = writeln!(self.out, "  {f} = load ptr, ptr @__zil_current_task");
                let _ = writeln!(self.out, "  {f_null} = icmp eq ptr {f}, null");
                let _ = writeln!(self.out, "  br i1 {f_null}, label %{l_ab}, label %{l_have}");
                let _ = writeln!(self.out, "{l_have}:");
                let _ = writeln!(
                    self.out,
                    "  {st} = getelementptr inbounds %__zil_task_hdr, ptr {f}, i32 0, i32 0"
                );
                let _ = writeln!(self.out, "  {cur_st} = load i32, ptr {st}");
                let _ = writeln!(self.out, "  {is_cl} = icmp eq i32 {cur_st}, 4");
                let _ = writeln!(self.out, "  {next_st} = select i1 {is_cl}, i32 4, i32 1");
                let _ = writeln!(self.out, "  store i32 {next_st}, ptr {st}");
                let _ = writeln!(
                    self.out,
                    "  {cctx} = getelementptr inbounds %__zil_task_hdr, ptr {f}, i32 0, i32 2"
                );
                if self.is_windows {
                    let cfib = self.fresh();
                    let _ = writeln!(self.out, "  {cfib} = load ptr, ptr {cctx}");
                    let _ = writeln!(self.out, "  call void @SwitchToFiber(ptr {cfib})");
                    let _ = writeln!(self.out, "  br label %{l_ok}");
                } else {
                    let wctx = self.fresh();
                    let sc_rc = self.fresh();
                    let sc_ok = self.fresh();
                    let _ = writeln!(
                        self.out,
                        "  {wctx} = getelementptr inbounds %__zil_task_hdr, ptr {f}, i32 0, i32 3"
                    );
                    let _ = writeln!(
                        self.out,
                        "  {sc_rc} = call i32 @swapcontext(ptr {wctx}, ptr {cctx})"
                    );
                    let _ = writeln!(self.out, "  {sc_ok} = icmp eq i32 {sc_rc}, 0");
                    let _ = writeln!(self.out, "  br i1 {sc_ok}, label %{l_ok}, label %{l_ab}");
                }
                let _ = writeln!(self.out, "{l_ab}:");
                let _ = writeln!(self.out, "  call void @abort()");
                let _ = writeln!(self.out, "  unreachable");
                let _ = writeln!(self.out, "{l_ok}:");
                Ok(())
            }
            IntrinsicOp::TaskCreate | IntrinsicOp::TaskStart => {
                if args.is_empty() {
                    return Err(BodyEmitError::new(
                        "TaskCreate/TaskStart missing worker argument",
                    ));
                }
                let (wfn_v, _, _) = self.emit_operand(&args[0])?;
                let mut arg_vals: Vec<(String, String, String, Option<TyId>)> =
                    Vec::with_capacity(args.len() - 1);
                for a in &args[1..] {
                    let (v, lt, aty) = self.emit_operand(a)?;
                    let st = if let Some(tid) = aty {
                        llvm_storage_ty(self.tys(), tid)?
                    } else if lt == "void" {
                        "i8".to_string()
                    } else {
                        lt.clone()
                    };
                    arg_vals.push((v, lt, st, aty));
                }
                let d_llvm = llvm_storage_ty(self.tys(), d_ty)?;
                let (task_res_ty, task_err_ty) = if matches!(op, IntrinsicOp::TaskStart) {
                    match self.tys().get(d_ty) {
                        Some(TyKind::Managed(inner)) => match self.tys().get(*inner) {
                            Some(TyKind::Opaque(OpaqueTy::Task { result, .. })) => (*result, None),
                            _ => {
                                return Err(BodyEmitError::new("TaskStart inner is not Task"));
                            }
                        },
                        _ => return Err(BodyEmitError::new("TaskStart dest is not Managed")),
                    }
                } else {
                    match self.tys().get(d_ty) {
                        Some(TyKind::ErrorUnion { ok, err }) => match self.tys().get(*ok) {
                            Some(TyKind::Managed(inner)) => match self.tys().get(*inner) {
                                Some(TyKind::Opaque(OpaqueTy::Task { result, .. })) => {
                                    (*result, Some(*err))
                                }
                                _ => {
                                    return Err(BodyEmitError::new(
                                        "TaskCreate ok inner is not Task",
                                    ));
                                }
                            },
                            _ => return Err(BodyEmitError::new("TaskCreate ok is not Managed")),
                        },
                        _ => return Err(BodyEmitError::new("TaskCreate dest is not ErrorUnion")),
                    }
                };
                let res_llvm = llvm_ty(self.tys(), task_res_ty)?;
                let res_stor = llvm_storage_ty(self.tys(), task_res_ty)?;

                let mut frame_fields = vec![
                    "%__zil_task_hdr".to_string(),
                    res_stor.clone(),
                    "ptr".to_string(),
                ];
                for (_, _, st, _) in &arg_vals {
                    frame_fields.push(st.clone());
                }
                let frame_ty = format!("{{ {} }}", frame_fields.join(", "));
                let tid = self.tmp;
                self.tmp += 1;
                let tramp_sym = format!("__zil_task_entry_f{}_{tid}", self.fn_id.0);

                // Emit worker trampoline into extra_defs.
                if self.is_windows {
                    let _ = writeln!(
                        self.extra_defs,
                        "\ndefine internal void @{tramp_sym}(ptr %frame) {{"
                    );
                    self.extra_defs.push_str("entry:\n");
                } else {
                    let _ = writeln!(
                        self.extra_defs,
                        "\ndefine internal void @{tramp_sym}(i32 %lo, i32 %hi) {{"
                    );
                    self.extra_defs.push_str("entry:\n");
                    self.extra_defs.push_str("  %lo64 = zext i32 %lo to i64\n");
                    self.extra_defs.push_str("  %hi64 = zext i32 %hi to i64\n");
                    self.extra_defs.push_str("  %hi_sh = shl i64 %hi64, 32\n");
                    self.extra_defs.push_str("  %addr = or i64 %hi_sh, %lo64\n");
                    self.extra_defs
                        .push_str("  %frame = inttoptr i64 %addr to ptr\n");
                }
                let _ = writeln!(
                    self.extra_defs,
                    "  %fn_slot = getelementptr inbounds {frame_ty}, ptr %frame, i32 0, i32 2"
                );
                self.extra_defs
                    .push_str("  %wfn = load ptr, ptr %fn_slot\n");
                let mut call_args = Vec::new();
                for (i, (_, lt, _, _)) in arg_vals.iter().enumerate() {
                    if lt != "void" {
                        let slot_idx = 3 + i;
                        let _ = writeln!(
                            self.extra_defs,
                            "  %ap{i} = getelementptr inbounds {frame_ty}, ptr %frame, i32 0, i32 {slot_idx}"
                        );
                        let _ = writeln!(self.extra_defs, "  %av{i} = load {lt}, ptr %ap{i}");
                        call_args.push(format!("{lt} %av{i}"));
                    }
                }
                self.extra_defs
                    .push_str("  store i1 false, ptr @__zil_task_cancel_trap\n");
                if res_llvm == "void" {
                    let _ = writeln!(
                        self.extra_defs,
                        "  call void %wfn({})",
                        call_args.join(", ")
                    );
                } else {
                    let _ = writeln!(
                        self.extra_defs,
                        "  %rv = call {res_llvm} %wfn({})",
                        call_args.join(", ")
                    );
                }
                self.extra_defs.push_str(
                    "  %st_slot = getelementptr inbounds %__zil_task_hdr, ptr %frame, i32 0, i32 0\n",
                );
                self.extra_defs
                    .push_str("  %end_st = load i32, ptr %st_slot\n");
                self.extra_defs
                    .push_str("  %was_canc = icmp eq i32 %end_st, 4\n");
                self.extra_defs
                    .push_str("  br i1 %was_canc, label %done_canc, label %done_ok\n");
                self.extra_defs.push_str("done_ok:\n");
                if res_llvm != "void" {
                    let _ = writeln!(
                        self.extra_defs,
                        "  %rslot = getelementptr inbounds {frame_ty}, ptr %frame, i32 0, i32 1"
                    );
                    let _ = writeln!(self.extra_defs, "  store {res_llvm} %rv, ptr %rslot");
                }
                self.extra_defs.push_str("  store i32 2, ptr %st_slot\n");
                self.extra_defs.push_str("  br label %swap_back\n");
                self.extra_defs.push_str("done_canc:\n");
                if res_llvm != "void" {
                    let saved_len = self.out.len();
                    let res = self.emit_drop_ptr("%canc_rslot", task_res_ty);
                    let drop_res_ir = self.out.split_off(saved_len);
                    res?;
                    if !drop_res_ir.is_empty() {
                        self.extra_defs
                            .push_str("  %canc_trapped = load i1, ptr @__zil_task_cancel_trap\n");
                        self.extra_defs
                            .push_str("  store i1 false, ptr @__zil_task_cancel_trap\n");
                        self.extra_defs.push_str(
                            "  br i1 %canc_trapped, label %done_canc_finish, label %done_canc_drop\n",
                        );
                        self.extra_defs.push_str("done_canc_drop:\n");
                        let _ = writeln!(
                            self.extra_defs,
                            "  %canc_rslot = getelementptr inbounds {frame_ty}, ptr %frame, i32 0, i32 1"
                        );
                        let _ =
                            writeln!(self.extra_defs, "  store {res_llvm} %rv, ptr %canc_rslot");
                        self.extra_defs.push_str(&drop_res_ir);
                        self.extra_defs.push_str("  br label %done_canc_finish\n");
                        self.extra_defs.push_str("done_canc_finish:\n");
                    }
                }
                self.extra_defs.push_str("  store i32 5, ptr %st_slot\n");
                self.extra_defs.push_str("  br label %swap_back\n");
                self.extra_defs.push_str("swap_back:\n");
                self.extra_defs.push_str(
                    "  %cctx = getelementptr inbounds %__zil_task_hdr, ptr %frame, i32 0, i32 2\n",
                );
                if self.is_windows {
                    self.extra_defs.push_str("  %cfib = load ptr, ptr %cctx\n");
                    self.extra_defs
                        .push_str("  call void @SwitchToFiber(ptr %cfib)\n");
                    self.extra_defs.push_str("  call void @abort()\n");
                    self.extra_defs.push_str("  unreachable\n}\n");
                } else {
                    self.extra_defs.push_str(
                        "  %wctx = getelementptr inbounds %__zil_task_hdr, ptr %frame, i32 0, i32 3\n",
                    );
                    self.extra_defs
                        .push_str("  %sc_rc = call i32 @swapcontext(ptr %wctx, ptr %cctx)\n");
                    self.extra_defs
                        .push_str("  %sc_ok = icmp eq i32 %sc_rc, 0\n");
                    self.extra_defs
                        .push_str("  br i1 %sc_ok, label %swap_ok, label %swap_ab\n");
                    self.extra_defs.push_str("swap_ab:\n");
                    self.extra_defs.push_str("  call void @abort()\n");
                    self.extra_defs.push_str("  unreachable\n");
                    self.extra_defs.push_str("swap_ok:\n");
                    self.extra_defs.push_str("  ret void\n}\n");
                }

                // Optionally emit drop_args helper for tasks cancelled before start.
                let mut drop_args_body = String::new();
                for (i, (_, lt, _, aty)) in arg_vals.iter().enumerate().rev() {
                    if lt != "void"
                        && let Some(arg_ty) = *aty
                    {
                        let slot_idx = 3 + i;
                        let ap = format!("%da_p{i}");
                        let saved_len = self.out.len();
                        let res = self.emit_drop_ptr(&ap, arg_ty);
                        let one_drop = self.out.split_off(saved_len);
                        res?;
                        if !one_drop.is_empty() {
                            let _ = writeln!(
                                drop_args_body,
                                "  {ap} = getelementptr inbounds {frame_ty}, ptr %frame, i32 0, i32 {slot_idx}"
                            );
                            drop_args_body.push_str(&one_drop);
                        }
                    }
                }
                let drop_args_sym = if !drop_args_body.is_empty() {
                    let sym = format!("__zil_task_drop_args_f{}_{tid}", self.fn_id.0);
                    let _ = writeln!(
                        self.extra_defs,
                        "\ndefine internal void @{sym}(ptr %frame) {{"
                    );
                    self.extra_defs.push_str("entry:\n");
                    self.extra_defs.push_str(&drop_args_body);
                    self.extra_defs.push_str("  ret void\n}\n");
                    Some(sym)
                } else {
                    None
                };

                // Allocate single frame via calloc(1, sizeof(frame_ty)).
                let sz_ptr = self.fresh();
                let sz_i64 = self.fresh();
                let frame = self.fresh();
                let is_null = self.fresh();
                let fail_lbl = format!("tc_fail_{tid}");
                let ok_lbl = format!("tc_ok_{tid}");
                let done_lbl = format!("tc_done_{tid}");
                let _ = writeln!(
                    self.out,
                    "  {sz_ptr} = getelementptr {frame_ty}, ptr null, i32 1"
                );
                let _ = writeln!(self.out, "  {sz_i64} = ptrtoint ptr {sz_ptr} to i64");
                let _ = writeln!(
                    self.out,
                    "  {frame} = call ptr @calloc(i64 1, i64 {sz_i64})"
                );
                let _ = writeln!(self.out, "  {is_null} = icmp eq ptr {frame}, null");
                let _ = writeln!(
                    self.out,
                    "  br i1 {is_null}, label %{fail_lbl}, label %{ok_lbl}"
                );

                let _ = writeln!(self.out, "{fail_lbl}:");
                if let Some(task_err_ty) = task_err_ty {
                    let e_tag = self.fresh();
                    let e_slot = self.fresh();
                    let _ = writeln!(
                        self.out,
                        "  {e_tag} = getelementptr inbounds {d_llvm}, ptr {d_ptr}, i32 0, i32 0"
                    );
                    let _ = writeln!(self.out, "  store i1 0, ptr {e_tag}");
                    let _ = writeln!(
                        self.out,
                        "  {e_slot} = getelementptr inbounds {d_llvm}, ptr {d_ptr}, i32 0, i32 2"
                    );
                    let owned_failure_inputs = match self.tys().get(task_err_ty) {
                        Some(TyKind::Adt { adt, args: eargs })
                            if self
                                .tys()
                                .get_adt(*adt)
                                .is_some_and(|def| def.name == "TaskCreateOwnedFailure") =>
                        {
                            self.tys().field_ty(
                                *adt,
                                zil_ir::body::VariantIdx(0),
                                zil_ir::body::FieldIdx(1),
                                *eargs,
                            )
                        }
                        _ => None,
                    };
                    if let Some(inputs_ty) = owned_failure_inputs {
                        let err_llvm = llvm_storage_ty(self.tys(), task_err_ty)?;
                        let reason_p = self.fresh();
                        let inputs_p = self.fresh();
                        let _ = writeln!(
                            self.out,
                            "  {reason_p} = getelementptr inbounds {err_llvm}, ptr {e_slot}, i32 0, i32 0"
                        );
                        let _ = writeln!(self.out, "  store i32 0, ptr {reason_p}");
                        let _ = writeln!(
                            self.out,
                            "  {inputs_p} = getelementptr inbounds {err_llvm}, ptr {e_slot}, i32 0, i32 1"
                        );
                        if arg_vals.is_empty() {
                            let _ = writeln!(self.out, "  store i8 0, ptr {inputs_p}");
                        } else {
                            let inputs_llvm = llvm_storage_ty(self.tys(), inputs_ty)?;
                            for (i, (v, lt, _, _)) in arg_vals.iter().enumerate() {
                                let fp = self.fresh();
                                let _ = writeln!(
                                    self.out,
                                    "  {fp} = getelementptr inbounds {inputs_llvm}, ptr {inputs_p}, i32 0, i32 {i}"
                                );
                                if lt == "void" {
                                    let _ = writeln!(self.out, "  store i8 0, ptr {fp}");
                                } else {
                                    let _ = writeln!(self.out, "  store {lt} {v}, ptr {fp}");
                                }
                            }
                        }
                    } else {
                        let _ = writeln!(self.out, "  store i32 0, ptr {e_slot}");
                        for (v, lt, _, aty) in arg_vals.iter().rev() {
                            if lt != "void"
                                && let Some(arg_ty) = *aty
                            {
                                let ap = self.fresh();
                                let saved_len = self.out.len();
                                let res = self.emit_drop_ptr(&ap, arg_ty);
                                let one_drop = self.out.split_off(saved_len);
                                res?;
                                if !one_drop.is_empty() {
                                    let _ = writeln!(self.out, "  {ap} = alloca {lt}");
                                    let _ = writeln!(self.out, "  store {lt} {v}, ptr {ap}");
                                    self.out.push_str(&one_drop);
                                }
                            }
                        }
                    }
                } else {
                    let _ = writeln!(self.out, "  store ptr null, ptr {d_ptr}");
                    for (v, lt, _, aty) in arg_vals.iter().rev() {
                        if lt != "void"
                            && let Some(arg_ty) = *aty
                        {
                            let ap = self.fresh();
                            let saved_len = self.out.len();
                            let res = self.emit_drop_ptr(&ap, arg_ty);
                            let one_drop = self.out.split_off(saved_len);
                            res?;
                            if !one_drop.is_empty() {
                                let _ = writeln!(self.out, "  {ap} = alloca {lt}");
                                let _ = writeln!(self.out, "  store {lt} {v}, ptr {ap}");
                                self.out.push_str(&one_drop);
                            }
                        }
                    }
                }
                let _ = writeln!(self.out, "  br label %{done_lbl}");

                let _ = writeln!(self.out, "{ok_lbl}:");
                let st_p = self.fresh();
                let tr_p = self.fresh();
                let da_p = self.fresh();
                let fn_p = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {st_p} = getelementptr inbounds %__zil_task_hdr, ptr {frame}, i32 0, i32 0"
                );
                let _ = writeln!(self.out, "  store i32 0, ptr {st_p}");
                let _ = writeln!(
                    self.out,
                    "  {tr_p} = getelementptr inbounds %__zil_task_hdr, ptr {frame}, i32 0, i32 1"
                );
                let _ = writeln!(self.out, "  store ptr @{tramp_sym}, ptr {tr_p}");
                let _ = writeln!(
                    self.out,
                    "  {da_p} = getelementptr inbounds %__zil_task_hdr, ptr {frame}, i32 0, i32 6"
                );
                if let Some(sym) = &drop_args_sym {
                    let _ = writeln!(self.out, "  store ptr @{sym}, ptr {da_p}");
                } else {
                    let _ = writeln!(self.out, "  store ptr null, ptr {da_p}");
                }
                let _ = writeln!(
                    self.out,
                    "  {fn_p} = getelementptr inbounds {frame_ty}, ptr {frame}, i32 0, i32 2"
                );
                let _ = writeln!(self.out, "  store ptr {wfn_v}, ptr {fn_p}");
                for (i, (v, lt, _, _)) in arg_vals.iter().enumerate() {
                    if lt != "void" {
                        let slot_idx = 3 + i;
                        let ap = self.fresh();
                        let _ = writeln!(
                            self.out,
                            "  {ap} = getelementptr inbounds {frame_ty}, ptr {frame}, i32 0, i32 {slot_idx}"
                        );
                        let _ = writeln!(self.out, "  store {lt} {v}, ptr {ap}");
                    }
                }
                if task_err_ty.is_some() {
                    let ok_tag = self.fresh();
                    let ok_slot = self.fresh();
                    let _ = writeln!(
                        self.out,
                        "  {ok_tag} = getelementptr inbounds {d_llvm}, ptr {d_ptr}, i32 0, i32 0"
                    );
                    let _ = writeln!(self.out, "  store i1 1, ptr {ok_tag}");
                    let _ = writeln!(
                        self.out,
                        "  {ok_slot} = getelementptr inbounds {d_llvm}, ptr {d_ptr}, i32 0, i32 1"
                    );
                    let _ = writeln!(self.out, "  store ptr {frame}, ptr {ok_slot}");
                } else {
                    let _ = writeln!(self.out, "  store ptr {frame}, ptr {d_ptr}");
                }
                let _ = writeln!(self.out, "  br label %{done_lbl}");

                let _ = writeln!(self.out, "{done_lbl}:");
                Ok(())
            }
            IntrinsicOp::TaskValid => {
                let (arg_v, _, arg_ty) = self.emit_operand(&args[0])?;
                let is_ref = arg_ty
                    .and_then(|t| self.tys().get(t))
                    .is_some_and(|k| matches!(k, TyKind::Ref { .. }));
                let frame = if is_ref {
                    let loaded = self.fresh();
                    let _ = writeln!(self.out, "  {loaded} = load ptr, ptr {arg_v}");
                    loaded
                } else {
                    arg_v
                };
                let ok = self.fresh();
                let _ = writeln!(self.out, "  {ok} = icmp ne ptr {frame}, null");
                let _ = writeln!(self.out, "  store i1 {ok}, ptr {d_ptr}");
                Ok(())
            }
            IntrinsicOp::TaskPoll => {
                let (ref_v, _, _) = self.emit_operand(&args[0])?;
                let d_llvm = llvm_storage_ty(self.tys(), d_ty)?;
                let (poll_ty, poll_res_ty) = match self.tys().get(d_ty) {
                    Some(TyKind::ErrorUnion { ok, .. }) => match self.tys().get(*ok) {
                        Some(TyKind::Adt { adt, args: pargs }) => {
                            let rty = self
                                .tys()
                                .field_ty(
                                    *adt,
                                    zil_ir::body::VariantIdx(1),
                                    zil_ir::body::FieldIdx(0),
                                    *pargs,
                                )
                                .ok_or_else(|| BodyEmitError::new("Poll.Ready field missing"))?;
                            (*ok, rty)
                        }
                        _ => return Err(BodyEmitError::new("TaskPoll ok is not Poll")),
                    },
                    _ => return Err(BodyEmitError::new("TaskPoll dest is not ErrorUnion")),
                };
                let poll_llvm = llvm_storage_ty(self.tys(), poll_ty)?;
                let res_llvm = llvm_ty(self.tys(), poll_res_ty)?;
                let res_stor = llvm_storage_ty(self.tys(), poll_res_ty)?;
                let res_frame_ty = format!("{{ %__zil_task_hdr, {res_stor} }}");

                let tid = self.tmp;
                self.tmp += 1;
                let frame = self.fresh();
                let st_p = self.fresh();
                let st = self.fresh();
                let _ = writeln!(self.out, "  {frame} = load ptr, ptr {ref_v}");
                let _ = writeln!(
                    self.out,
                    "  {st_p} = getelementptr inbounds %__zil_task_hdr, ptr {frame}, i32 0, i32 0"
                );
                let _ = writeln!(self.out, "  {st} = load i32, ptr {st_p}");
                let l_start = format!("tp_start_{tid}");
                let l_resume = format!("tp_resume_{tid}");
                let l_comp = format!("tp_comp_{tid}");
                let l_canc = format!("tp_canc_{tid}");
                let l_inv = format!("tp_inv_{tid}");
                let l_pend = format!("tp_pend_{tid}");
                let l_ready = format!("tp_ready_{tid}");
                let l_done = format!("tp_done_{tid}");
                let _ = writeln!(
                    self.out,
                    "  switch i32 {st}, label %{l_inv} [ i32 0, label %{l_start} i32 1, label %{l_resume} i32 2, label %{l_comp} i32 3, label %{l_canc} i32 4, label %{l_canc} ]"
                );

                // Cancelled -> err(0)
                let _ = writeln!(self.out, "{l_canc}:");
                let c_tag = self.fresh();
                let c_err = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {c_tag} = getelementptr inbounds {d_llvm}, ptr {d_ptr}, i32 0, i32 0"
                );
                let _ = writeln!(self.out, "  store i1 0, ptr {c_tag}");
                let _ = writeln!(
                    self.out,
                    "  {c_err} = getelementptr inbounds {d_llvm}, ptr {d_ptr}, i32 0, i32 2"
                );
                let _ = writeln!(self.out, "  store i32 0, ptr {c_err}");
                let _ = writeln!(self.out, "  br label %{l_done}");

                // Complete -> err(1)
                let _ = writeln!(self.out, "{l_comp}:");
                let k_tag = self.fresh();
                let k_err = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {k_tag} = getelementptr inbounds {d_llvm}, ptr {d_ptr}, i32 0, i32 0"
                );
                let _ = writeln!(self.out, "  store i1 0, ptr {k_tag}");
                let _ = writeln!(
                    self.out,
                    "  {k_err} = getelementptr inbounds {d_llvm}, ptr {d_ptr}, i32 0, i32 2"
                );
                let _ = writeln!(self.out, "  store i32 1, ptr {k_err}");
                let _ = writeln!(self.out, "  br label %{l_done}");

                // Invalid -> err(2)
                let _ = writeln!(self.out, "{l_inv}:");
                let i_tag = self.fresh();
                let i_err = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {i_tag} = getelementptr inbounds {d_llvm}, ptr {d_ptr}, i32 0, i32 0"
                );
                let _ = writeln!(self.out, "  store i1 0, ptr {i_tag}");
                let _ = writeln!(
                    self.out,
                    "  {i_err} = getelementptr inbounds {d_llvm}, ptr {d_ptr}, i32 0, i32 2"
                );
                let _ = writeln!(self.out, "  store i32 2, ptr {i_err}");
                let _ = writeln!(self.out, "  br label %{l_done}");

                // Start: allocate/initialize worker stack/fiber.
                let _ = writeln!(self.out, "{l_start}:");
                if self.is_windows {
                    let tr_slot = self.fresh();
                    let tr_fn = self.fresh();
                    let wfib0 = self.fresh();
                    let wf_ok = self.fresh();
                    let wctx0 = self.fresh();
                    let l_wf_ok = format!("tp_wfok_{tid}");
                    let _ = writeln!(
                        self.out,
                        "  {tr_slot} = getelementptr inbounds %__zil_task_hdr, ptr {frame}, i32 0, i32 1"
                    );
                    let _ = writeln!(self.out, "  {tr_fn} = load ptr, ptr {tr_slot}");
                    let _ = writeln!(
                        self.out,
                        "  {wfib0} = call ptr @CreateFiberEx(i64 16384, i64 131072, i32 1, ptr {tr_fn}, ptr {frame})"
                    );
                    let _ = writeln!(self.out, "  {wf_ok} = icmp ne ptr {wfib0}, null");
                    let _ = writeln!(
                        self.out,
                        "  br i1 {wf_ok}, label %{l_wf_ok}, label %{l_inv}"
                    );
                    let _ = writeln!(self.out, "{l_wf_ok}:");
                    let _ = writeln!(
                        self.out,
                        "  {wctx0} = getelementptr inbounds %__zil_task_hdr, ptr {frame}, i32 0, i32 3"
                    );
                    let _ = writeln!(self.out, "  store ptr {wfib0}, ptr {wctx0}");
                    let _ = writeln!(self.out, "  br label %{l_resume}");
                } else {
                    let wctx0 = self.fresh();
                    let link_p = self.fresh();
                    let sp_p = self.fresh();
                    let flg_p = self.fresh();
                    let sz_p = self.fresh();
                    let stk_raw = self.fresh();
                    let stk_int = self.fresh();
                    let stk_add = self.fresh();
                    let guard_int = self.fresh();
                    let guard_ptr = self.fresh();
                    let gp_slot = self.fresh();
                    let usable_stk = self.fresh();
                    let tr_slot = self.fresh();
                    let tr_fn = self.fresh();
                    let addr = self.fresh();
                    let lo = self.fresh();
                    let hi64 = self.fresh();
                    let hi = self.fresh();
                    let _ = writeln!(
                        self.out,
                        "  {wctx0} = getelementptr inbounds %__zil_task_hdr, ptr {frame}, i32 0, i32 3"
                    );
                    let gc_rc = self.fresh();
                    let gc_ok = self.fresh();
                    let l_gc_ok = format!("tp_gcok_{tid}");
                    let _ = writeln!(self.out, "  {gc_rc} = call i32 @getcontext(ptr {wctx0})");
                    let _ = writeln!(self.out, "  {gc_ok} = icmp eq i32 {gc_rc}, 0");
                    let _ = writeln!(
                        self.out,
                        "  br i1 {gc_ok}, label %{l_gc_ok}, label %{l_inv}"
                    );
                    let _ = writeln!(self.out, "{l_gc_ok}:");
                    let _ = writeln!(
                        self.out,
                        "  {link_p} = getelementptr inbounds i8, ptr {wctx0}, i64 8"
                    );
                    let _ = writeln!(self.out, "  store ptr null, ptr {link_p}");
                    let _ = writeln!(
                        self.out,
                        "  {stk_raw} = getelementptr inbounds %__zil_task_hdr, ptr {frame}, i32 0, i32 4"
                    );
                    let _ = writeln!(self.out, "  {stk_int} = ptrtoint ptr {stk_raw} to i64");
                    let _ = writeln!(self.out, "  {stk_add} = add i64 {stk_int}, 4095");
                    let _ = writeln!(self.out, "  {guard_int} = and i64 {stk_add}, -4096");
                    let _ = writeln!(self.out, "  {guard_ptr} = inttoptr i64 {guard_int} to ptr");
                    let mp_rc = self.fresh();
                    let mp_ok = self.fresh();
                    let l_mp_ok = format!("tp_mpok_{tid}");
                    let _ = writeln!(
                        self.out,
                        "  {mp_rc} = call i32 @mprotect(ptr {guard_ptr}, i64 4096, i32 0)"
                    );
                    let _ = writeln!(self.out, "  {mp_ok} = icmp eq i32 {mp_rc}, 0");
                    let _ = writeln!(
                        self.out,
                        "  br i1 {mp_ok}, label %{l_mp_ok}, label %{l_inv}"
                    );
                    let _ = writeln!(self.out, "{l_mp_ok}:");
                    let _ = writeln!(
                        self.out,
                        "  {gp_slot} = getelementptr inbounds %__zil_task_hdr, ptr {frame}, i32 0, i32 5"
                    );
                    let _ = writeln!(self.out, "  store ptr {guard_ptr}, ptr {gp_slot}");
                    let _ = writeln!(
                        self.out,
                        "  {usable_stk} = getelementptr inbounds i8, ptr {guard_ptr}, i64 4096"
                    );
                    let _ = writeln!(
                        self.out,
                        "  {sp_p} = getelementptr inbounds i8, ptr {wctx0}, i64 16"
                    );
                    let _ = writeln!(self.out, "  store ptr {usable_stk}, ptr {sp_p}");
                    let _ = writeln!(
                        self.out,
                        "  {flg_p} = getelementptr inbounds i8, ptr {wctx0}, i64 24"
                    );
                    let _ = writeln!(self.out, "  store i32 0, ptr {flg_p}");
                    let _ = writeln!(
                        self.out,
                        "  {sz_p} = getelementptr inbounds i8, ptr {wctx0}, i64 32"
                    );
                    let _ = writeln!(self.out, "  store i64 65536, ptr {sz_p}");
                    let _ = writeln!(
                        self.out,
                        "  {tr_slot} = getelementptr inbounds %__zil_task_hdr, ptr {frame}, i32 0, i32 1"
                    );
                    let _ = writeln!(self.out, "  {tr_fn} = load ptr, ptr {tr_slot}");
                    let _ = writeln!(self.out, "  {addr} = ptrtoint ptr {frame} to i64");
                    let _ = writeln!(self.out, "  {lo} = trunc i64 {addr} to i32");
                    let _ = writeln!(self.out, "  {hi64} = lshr i64 {addr}, 32");
                    let _ = writeln!(self.out, "  {hi} = trunc i64 {hi64} to i32");
                    let _ = writeln!(
                        self.out,
                        "  call void (ptr, ptr, i32, ...) @makecontext(ptr {wctx0}, ptr {tr_fn}, i32 2, i32 {lo}, i32 {hi})"
                    );
                    let _ = writeln!(self.out, "  br label %{l_resume}");
                }

                // Resume: switch into worker, then check state.
                let _ = writeln!(self.out, "{l_resume}:");
                let prev = self.fresh();
                let cctx = self.fresh();
                let wctx = self.fresh();
                let st2 = self.fresh();
                let is_ready = self.fresh();
                let _ = writeln!(self.out, "  {prev} = load ptr, ptr @__zil_current_task");
                if self.is_windows {
                    let cfib = self.fresh();
                    let wfib = self.fresh();
                    let _ = writeln!(self.out, "  {cfib} = call ptr @__zil_ensure_fiber()");
                    let _ = writeln!(
                        self.out,
                        "  {cctx} = getelementptr inbounds %__zil_task_hdr, ptr {frame}, i32 0, i32 2"
                    );
                    let _ = writeln!(self.out, "  store ptr {cfib}, ptr {cctx}");
                    let _ = writeln!(
                        self.out,
                        "  {wctx} = getelementptr inbounds %__zil_task_hdr, ptr {frame}, i32 0, i32 3"
                    );
                    let _ = writeln!(self.out, "  {wfib} = load ptr, ptr {wctx}");
                    let _ = writeln!(self.out, "  store ptr {frame}, ptr @__zil_current_task");
                    let _ = writeln!(self.out, "  store ptr {wfib}, ptr @__zil_current_fiber");
                    let _ = writeln!(self.out, "  call void @SwitchToFiber(ptr {wfib})");
                    let _ = writeln!(self.out, "  store ptr {cfib}, ptr @__zil_current_fiber");
                    let _ = writeln!(self.out, "  store ptr {prev}, ptr @__zil_current_task");
                } else {
                    let sc_rc = self.fresh();
                    let sc_ok = self.fresh();
                    let l_sc_ok = format!("tp_scok_{tid}");
                    let l_sc_ab = format!("tp_scab_{tid}");
                    let _ = writeln!(self.out, "  store ptr {frame}, ptr @__zil_current_task");
                    let _ = writeln!(
                        self.out,
                        "  {cctx} = getelementptr inbounds %__zil_task_hdr, ptr {frame}, i32 0, i32 2"
                    );
                    let _ = writeln!(
                        self.out,
                        "  {wctx} = getelementptr inbounds %__zil_task_hdr, ptr {frame}, i32 0, i32 3"
                    );
                    let _ = writeln!(
                        self.out,
                        "  {sc_rc} = call i32 @swapcontext(ptr {cctx}, ptr {wctx})"
                    );
                    let _ = writeln!(self.out, "  {sc_ok} = icmp eq i32 {sc_rc}, 0");
                    let _ = writeln!(
                        self.out,
                        "  br i1 {sc_ok}, label %{l_sc_ok}, label %{l_sc_ab}"
                    );
                    let _ = writeln!(self.out, "{l_sc_ab}:");
                    let _ = writeln!(self.out, "  call void @abort()");
                    let _ = writeln!(self.out, "  unreachable");
                    let _ = writeln!(self.out, "{l_sc_ok}:");
                    let _ = writeln!(self.out, "  store ptr {prev}, ptr @__zil_current_task");
                }
                let _ = writeln!(self.out, "  {st2} = load i32, ptr {st_p}");
                let _ = writeln!(self.out, "  {is_ready} = icmp eq i32 {st2}, 2");
                let _ = writeln!(
                    self.out,
                    "  br i1 {is_ready}, label %{l_ready}, label %{l_pend}"
                );

                // Pending -> ok(Poll.Pending)
                let _ = writeln!(self.out, "{l_pend}:");
                let p_tag = self.fresh();
                let p_ok = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {p_tag} = getelementptr inbounds {d_llvm}, ptr {d_ptr}, i32 0, i32 0"
                );
                let _ = writeln!(self.out, "  store i1 1, ptr {p_tag}");
                let _ = writeln!(
                    self.out,
                    "  {p_ok} = getelementptr inbounds {d_llvm}, ptr {d_ptr}, i32 0, i32 1"
                );
                let _ = writeln!(self.out, "  store i32 0, ptr {p_ok}");
                let _ = writeln!(self.out, "  br label %{l_done}");

                // Ready -> ok(Poll.Ready(result))
                let _ = writeln!(self.out, "{l_ready}:");
                let r_tag = self.fresh();
                let r_ok = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {r_tag} = getelementptr inbounds {d_llvm}, ptr {d_ptr}, i32 0, i32 0"
                );
                let _ = writeln!(self.out, "  store i1 1, ptr {r_tag}");
                let _ = writeln!(
                    self.out,
                    "  {r_ok} = getelementptr inbounds {d_llvm}, ptr {d_ptr}, i32 0, i32 1"
                );
                let _ = writeln!(self.out, "  store i32 1, ptr {r_ok}");
                if res_llvm != "void" {
                    let rsrc = self.fresh();
                    let rval = self.fresh();
                    let rdst = self.fresh();
                    let _ = writeln!(
                        self.out,
                        "  {rsrc} = getelementptr inbounds {res_frame_ty}, ptr {frame}, i32 0, i32 1"
                    );
                    let _ = writeln!(self.out, "  {rval} = load {res_llvm}, ptr {rsrc}");
                    let _ = writeln!(
                        self.out,
                        "  {rdst} = getelementptr inbounds {poll_llvm}, ptr {r_ok}, i32 0, i32 2, i32 0"
                    );
                    let _ = writeln!(self.out, "  store {res_llvm} {rval}, ptr {rdst}");
                }
                let _ = writeln!(self.out, "  br label %{l_done}");

                let _ = writeln!(self.out, "{l_done}:");
                Ok(())
            }
            IntrinsicOp::TaskRequestCancel => {
                let (ref_v, _, _) = self.emit_operand(&args[0])?;
                let tid = self.tmp;
                self.tmp += 1;
                let frame = self.fresh();
                let st_p = self.fresh();
                let st = self.fresh();
                let l_c0 = format!("trc_c0_{tid}");
                let l_c1 = format!("trc_c1_{tid}");
                let l_done = format!("trc_done_{tid}");
                let _ = writeln!(self.out, "  {frame} = load ptr, ptr {ref_v}");
                let _ = writeln!(
                    self.out,
                    "  {st_p} = getelementptr inbounds %__zil_task_hdr, ptr {frame}, i32 0, i32 0"
                );
                let _ = writeln!(self.out, "  {st} = load i32, ptr {st_p}");
                let _ = writeln!(
                    self.out,
                    "  switch i32 {st}, label %{l_done} [ i32 0, label %{l_c0} i32 1, label %{l_c1} ]"
                );
                let _ = writeln!(self.out, "{l_c0}:");
                let _ = writeln!(self.out, "  store i32 3, ptr {st_p}");
                let _ = writeln!(self.out, "  br label %{l_done}");
                let _ = writeln!(self.out, "{l_c1}:");
                let _ = writeln!(self.out, "  store i32 4, ptr {st_p}");
                let _ = writeln!(self.out, "  br label %{l_done}");
                let _ = writeln!(self.out, "{l_done}:");
                Ok(())
            }
            IntrinsicOp::TaskPollClose => {
                let (ref_v, _, _) = self.emit_operand(&args[0])?;
                let tid = self.tmp;
                self.tmp += 1;
                let frame = self.fresh();
                let st_p = self.fresh();
                let st = self.fresh();
                let l_unstarted = format!("tpc_un_{tid}");
                let l_call_da = format!("tpc_da_{tid}");
                let l_mark_closed = format!("tpc_mc_{tid}");
                let l_mark_close = format!("tpc_cl_{tid}");
                let l_drive = format!("tpc_drv_{tid}");
                let l_pend = format!("tpc_pnd_{tid}");
                let l_ready = format!("tpc_rdy_{tid}");
                let l_done = format!("tpc_done_{tid}");
                let _ = writeln!(self.out, "  {frame} = load ptr, ptr {ref_v}");
                let _ = writeln!(
                    self.out,
                    "  {st_p} = getelementptr inbounds %__zil_task_hdr, ptr {frame}, i32 0, i32 0"
                );
                let _ = writeln!(self.out, "  {st} = load i32, ptr {st_p}");
                let _ = writeln!(
                    self.out,
                    "  switch i32 {st}, label %{l_ready} [ i32 0, label %{l_unstarted} i32 3, label %{l_unstarted} i32 2, label %{l_mark_closed} i32 1, label %{l_mark_close} i32 4, label %{l_drive} ]"
                );

                // Unstarted (0 or 3): drop owned args once, then mark Closed (5).
                let _ = writeln!(self.out, "{l_unstarted}:");
                let da_slot = self.fresh();
                let da_fn = self.fresh();
                let has_da = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {da_slot} = getelementptr inbounds %__zil_task_hdr, ptr {frame}, i32 0, i32 6"
                );
                let _ = writeln!(self.out, "  {da_fn} = load ptr, ptr {da_slot}");
                let _ = writeln!(self.out, "  {has_da} = icmp ne ptr {da_fn}, null");
                let _ = writeln!(
                    self.out,
                    "  br i1 {has_da}, label %{l_call_da}, label %{l_mark_closed}"
                );
                let _ = writeln!(self.out, "{l_call_da}:");
                let _ = writeln!(self.out, "  store ptr null, ptr {da_slot}");
                let _ = writeln!(self.out, "  call void {da_fn}(ptr {frame})");
                let _ = writeln!(self.out, "  br label %{l_mark_closed}");

                let _ = writeln!(self.out, "{l_mark_closed}:");
                let _ = writeln!(self.out, "  store i32 5, ptr {st_p}");
                let _ = writeln!(self.out, "  br label %{l_ready}");

                // Suspended (1): transition to Closing (4) and drive worker unwind.
                let _ = writeln!(self.out, "{l_mark_close}:");
                let _ = writeln!(self.out, "  store i32 4, ptr {st_p}");
                let _ = writeln!(self.out, "  br label %{l_drive}");

                // Closing (4): resume worker on cancel path; return Ready iff Closed (5).
                let _ = writeln!(self.out, "{l_drive}:");
                let prev = self.fresh();
                let cctx = self.fresh();
                let wctx = self.fresh();
                let st2 = self.fresh();
                let is_closed = self.fresh();
                let _ = writeln!(self.out, "  {prev} = load ptr, ptr @__zil_current_task");
                if self.is_windows {
                    let cfib = self.fresh();
                    let wfib = self.fresh();
                    let _ = writeln!(self.out, "  {cfib} = call ptr @__zil_ensure_fiber()");
                    let _ = writeln!(
                        self.out,
                        "  {cctx} = getelementptr inbounds %__zil_task_hdr, ptr {frame}, i32 0, i32 2"
                    );
                    let _ = writeln!(self.out, "  store ptr {cfib}, ptr {cctx}");
                    let _ = writeln!(
                        self.out,
                        "  {wctx} = getelementptr inbounds %__zil_task_hdr, ptr {frame}, i32 0, i32 3"
                    );
                    let _ = writeln!(self.out, "  {wfib} = load ptr, ptr {wctx}");
                    let _ = writeln!(self.out, "  store ptr {frame}, ptr @__zil_current_task");
                    let _ = writeln!(self.out, "  store ptr {wfib}, ptr @__zil_current_fiber");
                    let _ = writeln!(self.out, "  call void @SwitchToFiber(ptr {wfib})");
                    let _ = writeln!(self.out, "  store ptr {cfib}, ptr @__zil_current_fiber");
                    let _ = writeln!(self.out, "  store ptr {prev}, ptr @__zil_current_task");
                } else {
                    let _ = writeln!(self.out, "  store ptr {frame}, ptr @__zil_current_task");
                    let _ = writeln!(
                        self.out,
                        "  {cctx} = getelementptr inbounds %__zil_task_hdr, ptr {frame}, i32 0, i32 2"
                    );
                    let _ = writeln!(
                        self.out,
                        "  {wctx} = getelementptr inbounds %__zil_task_hdr, ptr {frame}, i32 0, i32 3"
                    );
                    let sc_rc = self.fresh();
                    let sc_ok = self.fresh();
                    let l_sc_ok = format!("tpc_scok_{tid}");
                    let l_sc_ab = format!("tpc_scab_{tid}");
                    let _ = writeln!(
                        self.out,
                        "  {sc_rc} = call i32 @swapcontext(ptr {cctx}, ptr {wctx})"
                    );
                    let _ = writeln!(self.out, "  {sc_ok} = icmp eq i32 {sc_rc}, 0");
                    let _ = writeln!(
                        self.out,
                        "  br i1 {sc_ok}, label %{l_sc_ok}, label %{l_sc_ab}"
                    );
                    let _ = writeln!(self.out, "{l_sc_ab}:");
                    let _ = writeln!(self.out, "  call void @abort()");
                    let _ = writeln!(self.out, "  unreachable");
                    let _ = writeln!(self.out, "{l_sc_ok}:");
                    let _ = writeln!(self.out, "  store ptr {prev}, ptr @__zil_current_task");
                }
                let _ = writeln!(self.out, "  {st2} = load i32, ptr {st_p}");
                let _ = writeln!(self.out, "  {is_closed} = icmp eq i32 {st2}, 5");
                let _ = writeln!(
                    self.out,
                    "  br i1 {is_closed}, label %{l_ready}, label %{l_pend}"
                );

                // Poll.Pending -> enum tag 0
                let _ = writeln!(self.out, "{l_pend}:");
                let _ = writeln!(self.out, "  store i32 0, ptr {d_ptr}");
                let _ = writeln!(self.out, "  br label %{l_done}");

                // Poll.Ready(()) -> enum tag 1
                let _ = writeln!(self.out, "{l_ready}:");
                let _ = writeln!(self.out, "  store i32 1, ptr {d_ptr}");
                let _ = writeln!(self.out, "  br label %{l_done}");

                let _ = writeln!(self.out, "{l_done}:");
                Ok(())
            }
            IntrinsicOp::TaskFinishDestroy => {
                let (frame, _, _) = self.emit_operand(&args[0])?;
                let tid = self.tmp;
                self.tmp += 1;
                let st_p = self.fresh();
                let st = self.fresh();
                let is_closed = self.fresh();
                let l_abort = format!("tfd_ab_{tid}");
                let l_ok = format!("tfd_ok_{tid}");
                let l_unprot = format!("tfd_up_{tid}");
                let l_free = format!("tfd_fr_{tid}");
                let _ = writeln!(
                    self.out,
                    "  {st_p} = getelementptr inbounds %__zil_task_hdr, ptr {frame}, i32 0, i32 0"
                );
                let _ = writeln!(self.out, "  {st} = load i32, ptr {st_p}");
                let _ = writeln!(self.out, "  {is_closed} = icmp eq i32 {st}, 5");
                let _ = writeln!(
                    self.out,
                    "  br i1 {is_closed}, label %{l_ok}, label %{l_abort}"
                );
                let _ = writeln!(self.out, "{l_abort}:");
                let _ = writeln!(self.out, "  call void @abort()");
                let _ = writeln!(self.out, "  unreachable");
                let _ = writeln!(self.out, "{l_ok}:");
                let _ = writeln!(self.out, "  store i32 6, ptr {st_p}");
                if self.is_windows {
                    let wf_slot = self.fresh();
                    let wf = self.fresh();
                    let has_wf = self.fresh();
                    let _ = writeln!(
                        self.out,
                        "  {wf_slot} = getelementptr inbounds %__zil_task_hdr, ptr {frame}, i32 0, i32 3"
                    );
                    let _ = writeln!(self.out, "  {wf} = load ptr, ptr {wf_slot}");
                    let _ = writeln!(self.out, "  {has_wf} = icmp ne ptr {wf}, null");
                    let _ = writeln!(
                        self.out,
                        "  br i1 {has_wf}, label %{l_unprot}, label %{l_free}"
                    );
                    let _ = writeln!(self.out, "{l_unprot}:");
                    let _ = writeln!(self.out, "  store ptr null, ptr {wf_slot}");
                    let _ = writeln!(self.out, "  call void @DeleteFiber(ptr {wf})");
                    let _ = writeln!(self.out, "  br label %{l_free}");
                } else {
                    let gp_slot = self.fresh();
                    let gp = self.fresh();
                    let has_gp = self.fresh();
                    let _ = writeln!(
                        self.out,
                        "  {gp_slot} = getelementptr inbounds %__zil_task_hdr, ptr {frame}, i32 0, i32 5"
                    );
                    let _ = writeln!(self.out, "  {gp} = load ptr, ptr {gp_slot}");
                    let _ = writeln!(self.out, "  {has_gp} = icmp ne ptr {gp}, null");
                    let _ = writeln!(
                        self.out,
                        "  br i1 {has_gp}, label %{l_unprot}, label %{l_free}"
                    );
                    let _ = writeln!(self.out, "{l_unprot}:");
                    let up_rc = self.fresh();
                    let up_ok = self.fresh();
                    let _ = writeln!(
                        self.out,
                        "  {up_rc} = call i32 @mprotect(ptr {gp}, i64 4096, i32 3)"
                    );
                    let _ = writeln!(self.out, "  {up_ok} = icmp eq i32 {up_rc}, 0");
                    let _ = writeln!(
                        self.out,
                        "  br i1 {up_ok}, label %{l_free}, label %{l_abort}"
                    );
                }
                let _ = writeln!(self.out, "{l_free}:");
                let _ = writeln!(self.out, "  call void @free(ptr {frame})");
                Ok(())
            }
            IntrinsicOp::OwnedDynCreate => {
                let arg0 = args
                    .first()
                    .ok_or_else(|| BodyEmitError::new("OwnedDynCreate missing value operand"))?;
                let (v0, lt0, concrete_ty_opt) = self.emit_operand(arg0)?;
                let concrete_ty = concrete_ty_opt
                    .ok_or_else(|| BodyEmitError::new("OwnedDynCreate missing concrete type"))?;
                let dyn_id = if let Some(TyKind::ErrorUnion { ok, .. }) = self.tys().get(d_ty) {
                    if let Some(TyKind::OwnedDyn { def: id, .. }) = self.tys().get(*ok) {
                        *id
                    } else if let Some(TyKind::Managed(inner)) = self.tys().get(*ok)
                        && let Some(TyKind::OwnedDyn { def: id, .. }) = self.tys().get(*inner)
                    {
                        *id
                    } else {
                        return Err(BodyEmitError::new("OwnedDynCreate ok arm is not OwnedDyn"));
                    }
                } else {
                    return Err(BodyEmitError::new(
                        "OwnedDynCreate destination is not ErrorUnion",
                    ));
                };
                let vt_idx = self
                    .module
                    .dyn_impls
                    .iter()
                    .position(|d| d.dyn_id == dyn_id && d.concrete_ty == concrete_ty)
                    .ok_or_else(|| {
                        BodyEmitError::new(format!(
                            "OwnedDynCreate missing DynImpl for ({dyn_id:?}, {concrete_ty:?})"
                        ))
                    })?;
                let d_llvm = llvm_ty(self.tys(), d_ty)?;
                let c_stor = llvm_storage_ty(self.tys(), concrete_ty)?;
                let tmp_slot = self.alloc_entry(&c_stor);
                if lt0 != "void" {
                    let _ = writeln!(self.out, "  store {lt0} {v0}, ptr {tmp_slot}");
                }
                let sz = if lt0 == "void" {
                    "1".to_string()
                } else {
                    let gep = self.fresh();
                    let sz_raw = self.fresh();
                    let is_zero = self.fresh();
                    let sz_nz = self.fresh();
                    let _ = writeln!(
                        self.out,
                        "  {gep} = getelementptr {c_stor}, ptr null, i32 1"
                    );
                    let _ = writeln!(self.out, "  {sz_raw} = ptrtoint ptr {gep} to i64");
                    let _ = writeln!(self.out, "  {is_zero} = icmp eq i64 {sz_raw}, 0");
                    let _ = writeln!(
                        self.out,
                        "  {sz_nz} = select i1 {is_zero}, i64 1, i64 {sz_raw}"
                    );
                    sz_nz
                };
                let mem = self.fresh();
                let is_null = self.fresh();
                let l_fail = format!("od_fail_{}", self.tmp);
                let l_ok = format!("od_ok_{}", self.tmp);
                let l_done = format!("od_done_{}", self.tmp);
                self.tmp += 1;
                let _ = writeln!(self.out, "  {mem} = call ptr @calloc(i64 1, i64 {sz})");
                let _ = writeln!(self.out, "  {is_null} = icmp eq ptr {mem}, null");
                let _ = writeln!(
                    self.out,
                    "  br i1 {is_null}, label %{l_fail}, label %{l_ok}"
                );
                let _ = writeln!(self.out, "{l_fail}:");
                // Drop the concrete value on allocation failure so resources never leak.
                self.emit_drop_ptr(&tmp_slot, concrete_ty)?;
                let err_val = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {err_val} = insertvalue {d_llvm} zeroinitializer, i1 false, 0"
                );
                let _ = writeln!(self.out, "  store {d_llvm} {err_val}, ptr {d_ptr}");
                let _ = writeln!(self.out, "  br label %{l_done}");
                let _ = writeln!(self.out, "{l_ok}:");
                if lt0 != "void" {
                    let _ = writeln!(self.out, "  store {lt0} {v0}, ptr {mem}");
                }
                let fat1 = self.fresh();
                let fat2 = self.fresh();
                let ok1 = self.fresh();
                let ok2 = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {fat1} = insertvalue {{ ptr, ptr }} undef, ptr {mem}, 0"
                );
                let _ = writeln!(
                    self.out,
                    "  {fat2} = insertvalue {{ ptr, ptr }} {fat1}, ptr @__zil_vtable_{vt_idx}, 1"
                );
                let _ = writeln!(
                    self.out,
                    "  {ok1} = insertvalue {d_llvm} zeroinitializer, i1 true, 0"
                );
                let _ = writeln!(
                    self.out,
                    "  {ok2} = insertvalue {d_llvm} {ok1}, {{ ptr, ptr }} {fat2}, 1"
                );
                let _ = writeln!(self.out, "  store {d_llvm} {ok2}, ptr {d_ptr}");
                let _ = writeln!(self.out, "  br label %{l_done}");
                let _ = writeln!(self.out, "{l_done}:");
                Ok(())
            }
            IntrinsicOp::RegistrationStart => {
                if args.len() != 3 {
                    return Err(BodyEmitError::new(
                        "RegistrationStart expects 3 arguments (reg, close, env)",
                    ));
                }
                let (reg_fn, _, _) = self.emit_operand(&args[0])?;
                let (close_fn, _, _) = self.emit_operand(&args[1])?;
                let (env_val, env_lt, env_ty_opt) = self.emit_operand(&args[2])?;
                let env_ty = env_ty_opt.ok_or_else(|| {
                    BodyEmitError::new("RegistrationStart missing environment type")
                })?;
                let env_st = if env_lt == "void" {
                    "i8".to_string()
                } else {
                    llvm_storage_ty(self.tys(), env_ty)?
                };
                let tid = self.tmp;
                self.tmp += 1;
                let frame_ty = format!("{{ %__zil_reg_hdr, {env_st} }}");

                // Resolve callable entry (closure body or fn ptr).
                let tramp_sym = format!("__zil_reg_tramp_f{}_{tid}", self.fn_id.0);
                let closure_sym = match self.tys().get(env_ty) {
                    Some(TyKind::Closure { def, .. }) => {
                        let c_fn_id = self
                            .module
                            .functions
                            .iter()
                            .position(|f| f.body.closure == Some(*def))
                            .map(FnId::from_index)
                            .ok_or_else(|| {
                                BodyEmitError::new("RegistrationStart closure body missing")
                            })?;
                        Some(fn_symbol(c_fn_id, &self.module.functions[c_fn_id.index()]))
                    }
                    Some(TyKind::FnPtr { .. }) => None,
                    _ => {
                        return Err(BodyEmitError::new(
                            "RegistrationStart environment must be a closure or fn ptr",
                        ));
                    }
                };
                let _ = writeln!(
                    self.extra_defs,
                    "\ndefine internal i64 @{tramp_sym}(ptr %ctx, i32 %arg) {{"
                );
                self.extra_defs.push_str(
                    "entry:\n\
  %is_null = icmp eq ptr %ctx, null\n\
  br i1 %is_null, label %reject, label %check\n\
check:\n\
  %gate_p = getelementptr inbounds %__zil_reg_hdr, ptr %ctx, i32 0, i32 0\n\
  %g = load i32, ptr %gate_p\n\
  %is_open = icmp eq i32 %g, 0\n\
  br i1 %is_open, label %run, label %reject\n\
run:\n\
  store i32 1, ptr %gate_p\n",
                );
                let _ = writeln!(
                    self.extra_defs,
                    "  %env_p = getelementptr inbounds {frame_ty}, ptr %ctx, i32 0, i32 1"
                );
                if let Some(csym) = closure_sym {
                    let _ = writeln!(
                        self.extra_defs,
                        "  %rv = call i32 @{csym}(ptr %env_p, i32 %arg)"
                    );
                } else {
                    self.extra_defs
                        .push_str("  %f_val = load ptr, ptr %env_p\n");
                    self.extra_defs
                        .push_str("  %rv = call i32 %f_val(i32 %arg)\n");
                }
                self.extra_defs.push_str(
                    "  %g_after = load i32, ptr %gate_p\n\
  %still_busy = icmp eq i32 %g_after, 1\n\
  br i1 %still_busy, label %restore, label %ret_ok\n\
restore:\n\
  store i32 0, ptr %gate_p\n\
  br label %ret_ok\n\
ret_ok:\n\
  %rv64 = zext i32 %rv to i64\n\
  ret i64 %rv64\n\
reject:\n\
  ret i64 4294967296\n\
}\n",
                );

                // Emit env drop helper if environment owns resources.
                let saved_len = self.out.len();
                let res = self.emit_drop_ptr("%env_p", env_ty);
                let env_drop_body = self.out.split_off(saved_len);
                res?;
                let drop_sym = if !env_drop_body.is_empty() {
                    let sym = format!("__zil_reg_drop_env_f{}_{tid}", self.fn_id.0);
                    let _ = writeln!(
                        self.extra_defs,
                        "\ndefine internal void @{sym}(ptr %frame) {{"
                    );
                    self.extra_defs.push_str("entry:\n");
                    let _ = writeln!(
                        self.extra_defs,
                        "  %env_p = getelementptr inbounds {frame_ty}, ptr %frame, i32 0, i32 1"
                    );
                    self.extra_defs.push_str(&env_drop_body);
                    self.extra_defs.push_str("  ret void\n}\n");
                    Some(sym)
                } else {
                    None
                };

                // Allocate frame via calloc(1, sizeof(frame_ty)).
                let sz_ptr = self.fresh();
                let sz_i64 = self.fresh();
                let raw = self.fresh();
                let is_oom = self.fresh();
                let l_oom = format!("reg_oom_{tid}");
                let l_init = format!("reg_init_{tid}");
                let l_fail = format!("reg_fail_{tid}");
                let l_ok = format!("reg_ok_{tid}");
                let l_done = format!("reg_done_{tid}");
                let _ = writeln!(
                    self.out,
                    "  {sz_ptr} = getelementptr {frame_ty}, ptr null, i32 1"
                );
                let _ = writeln!(self.out, "  {sz_i64} = ptrtoint ptr {sz_ptr} to i64");
                let _ = writeln!(self.out, "  {raw} = call ptr @calloc(i64 1, i64 {sz_i64})");
                let _ = writeln!(self.out, "  {is_oom} = icmp eq ptr {raw}, null");
                let _ = writeln!(
                    self.out,
                    "  br i1 {is_oom}, label %{l_oom}, label %{l_init}"
                );

                // OOM: drop untransferred env in place, store null handle.
                let _ = writeln!(self.out, "{l_oom}:");
                if env_lt != "void" && drop_sym.is_some() {
                    let tmp_env = self.fresh();
                    let _ = writeln!(self.out, "  {tmp_env} = alloca {env_st}");
                    let _ = writeln!(self.out, "  store {env_lt} {env_val}, ptr {tmp_env}");
                    self.emit_drop_ptr(&tmp_env, env_ty)?;
                }
                let _ = writeln!(self.out, "  store ptr null, ptr {d_ptr}");
                let _ = writeln!(self.out, "  br label %{l_done}");

                // Initialize frame header + env, then call C register_fn(trampoline, frame).
                let _ = writeln!(self.out, "{l_init}:");
                let gate_p = self.fresh();
                let cfn_p = self.fresh();
                let dfn_p = self.fresh();
                let env_p = self.fresh();
                let tok = self.fresh();
                let tok_null = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {gate_p} = getelementptr inbounds %__zil_reg_hdr, ptr {raw}, i32 0, i32 0"
                );
                let _ = writeln!(self.out, "  store i32 0, ptr {gate_p}");
                let _ = writeln!(
                    self.out,
                    "  {cfn_p} = getelementptr inbounds %__zil_reg_hdr, ptr {raw}, i32 0, i32 2"
                );
                let _ = writeln!(self.out, "  store ptr {close_fn}, ptr {cfn_p}");
                let _ = writeln!(
                    self.out,
                    "  {dfn_p} = getelementptr inbounds %__zil_reg_hdr, ptr {raw}, i32 0, i32 3"
                );
                if let Some(dsym) = &drop_sym {
                    let _ = writeln!(self.out, "  store ptr @{dsym}, ptr {dfn_p}");
                } else {
                    let _ = writeln!(self.out, "  store ptr null, ptr {dfn_p}");
                }
                if env_lt != "void" {
                    let _ = writeln!(
                        self.out,
                        "  {env_p} = getelementptr inbounds {frame_ty}, ptr {raw}, i32 0, i32 1"
                    );
                    let _ = writeln!(self.out, "  store {env_lt} {env_val}, ptr {env_p}");
                }
                let _ = writeln!(
                    self.out,
                    "  {tok} = call ptr {reg_fn}(ptr @{tramp_sym}, ptr {raw})"
                );
                let _ = writeln!(self.out, "  {tok_null} = icmp eq ptr {tok}, null");
                let _ = writeln!(
                    self.out,
                    "  br i1 {tok_null}, label %{l_fail}, label %{l_ok}"
                );

                // Install failure: close gate, drop env once, free frame, store null handle.
                let _ = writeln!(self.out, "{l_fail}:");
                let _ = writeln!(self.out, "  store i32 2, ptr {gate_p}");
                if let Some(dsym) = &drop_sym {
                    let _ = writeln!(self.out, "  store ptr null, ptr {dfn_p}");
                    let _ = writeln!(self.out, "  call void @{dsym}(ptr {raw})");
                }
                let _ = writeln!(self.out, "  call void @free(ptr {raw})");
                let _ = writeln!(self.out, "  store ptr null, ptr {d_ptr}");
                let _ = writeln!(self.out, "  br label %{l_done}");

                // Success: store token and return frame handle.
                let _ = writeln!(self.out, "{l_ok}:");
                let tok_p = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {tok_p} = getelementptr inbounds %__zil_reg_hdr, ptr {raw}, i32 0, i32 1"
                );
                let _ = writeln!(self.out, "  store ptr {tok}, ptr {tok_p}");
                let _ = writeln!(self.out, "  store ptr {raw}, ptr {d_ptr}");
                let _ = writeln!(self.out, "  br label %{l_done}");

                let _ = writeln!(self.out, "{l_done}:");
                Ok(())
            }
            IntrinsicOp::RegistrationValid => {
                let (frame, _, _) = self.emit_operand(&args[0])?;
                let ok = self.fresh();
                let _ = writeln!(self.out, "  {ok} = icmp ne ptr {frame}, null");
                let _ = writeln!(self.out, "  store i1 {ok}, ptr {d_ptr}");
                Ok(())
            }
            IntrinsicOp::RegistrationClose => {
                let (frame, _, _) = self.emit_operand(&args[0])?;
                let _ = writeln!(self.out, "  call void @__zil_reg_drop_frame(ptr {frame})");
                Ok(())
            }
            IntrinsicOp::AsyncRegistrationStart => {
                if args.len() != 4 {
                    return Err(BodyEmitError::new(
                        "AsyncRegistrationStart expects 4 arguments (reg, req, drain, env)",
                    ));
                }
                let (reg_fn, _, _) = self.emit_operand(&args[0])?;
                let (req_fn, _, _) = self.emit_operand(&args[1])?;
                let (drain_fn, _, _) = self.emit_operand(&args[2])?;
                let (env_val, env_lt, env_ty_opt) = self.emit_operand(&args[3])?;
                let env_ty = env_ty_opt.ok_or_else(|| {
                    BodyEmitError::new("AsyncRegistrationStart missing environment type")
                })?;
                let env_st = if env_lt == "void" {
                    "i8".to_string()
                } else {
                    llvm_storage_ty(self.tys(), env_ty)?
                };
                let tid = self.tmp;
                self.tmp += 1;
                let frame_ty = format!("{{ %__zil_areg_hdr, {env_st} }}");

                let tramp_sym = format!("__zil_areg_tramp_f{}_{tid}", self.fn_id.0);
                let closure_sym = match self.tys().get(env_ty) {
                    Some(TyKind::Closure { def, .. }) => {
                        let c_fn_id = self
                            .module
                            .functions
                            .iter()
                            .position(|f| f.body.closure == Some(*def))
                            .map(FnId::from_index)
                            .ok_or_else(|| {
                                BodyEmitError::new("AsyncRegistrationStart closure body missing")
                            })?;
                        Some(fn_symbol(c_fn_id, &self.module.functions[c_fn_id.index()]))
                    }
                    Some(TyKind::FnPtr { .. }) => None,
                    _ => {
                        return Err(BodyEmitError::new(
                            "AsyncRegistrationStart environment must be a closure or fn ptr",
                        ));
                    }
                };

                // Atomic cross-thread trampoline: single acq_rel CAS OpenIdle(0)->OpenBusy(1),
                // and atomicrmw and -2 release (OpenBusy(1)->OpenIdle(0), ClosingBusy(3)->ClosingIdle(2)).
                let _ = writeln!(
                    self.extra_defs,
                    "\ndefine internal i64 @{tramp_sym}(ptr %ctx, i32 %arg) {{"
                );
                self.extra_defs.push_str(
                    "entry:\n\
  %is_null = icmp eq ptr %ctx, null\n\
  br i1 %is_null, label %reject, label %admit\n\
admit:\n\
  %gate_p = getelementptr inbounds %__zil_areg_hdr, ptr %ctx, i32 0, i32 0\n\
  %cas = cmpxchg ptr %gate_p, i32 0, i32 1 acq_rel acquire\n\
  %ok = extractvalue { i32, i1 } %cas, 1\n\
  br i1 %ok, label %run, label %reject\n\
run:\n",
                );
                let _ = writeln!(
                    self.extra_defs,
                    "  %env_p = getelementptr inbounds {frame_ty}, ptr %ctx, i32 0, i32 1"
                );
                if let Some(csym) = closure_sym {
                    let _ = writeln!(
                        self.extra_defs,
                        "  %rv = call i32 @{csym}(ptr %env_p, i32 %arg)"
                    );
                } else {
                    self.extra_defs
                        .push_str("  %f_val = load ptr, ptr %env_p\n");
                    self.extra_defs
                        .push_str("  %rv = call i32 %f_val(i32 %arg)\n");
                }
                self.extra_defs.push_str(
                    "  %prev_g = atomicrmw and ptr %gate_p, i32 -2 acq_rel, align 4\n\
  %rv64 = zext i32 %rv to i64\n\
  ret i64 %rv64\n\
reject:\n\
  ret i64 4294967296\n\
}\n",
                );

                // Emit env drop helper if environment owns resources.
                let saved_len = self.out.len();
                let res = self.emit_drop_ptr("%env_p", env_ty);
                let env_drop_body = self.out.split_off(saved_len);
                res?;
                let drop_sym = if !env_drop_body.is_empty() {
                    let sym = format!("__zil_areg_drop_env_f{}_{tid}", self.fn_id.0);
                    let _ = writeln!(
                        self.extra_defs,
                        "\ndefine internal void @{sym}(ptr %frame) {{"
                    );
                    self.extra_defs.push_str("entry:\n");
                    let _ = writeln!(
                        self.extra_defs,
                        "  %env_p = getelementptr inbounds {frame_ty}, ptr %frame, i32 0, i32 1"
                    );
                    self.extra_defs.push_str(&env_drop_body);
                    self.extra_defs.push_str("  ret void\n}\n");
                    Some(sym)
                } else {
                    None
                };

                // Allocate frame via calloc(1, sizeof(frame_ty)).
                let sz_ptr = self.fresh();
                let sz_i64 = self.fresh();
                let raw = self.fresh();
                let is_oom = self.fresh();
                let l_oom = format!("areg_oom_{tid}");
                let l_init = format!("areg_init_{tid}");
                let l_fail = format!("areg_fail_{tid}");
                let l_ok = format!("areg_ok_{tid}");
                let l_done = format!("areg_done_{tid}");
                let _ = writeln!(
                    self.out,
                    "  {sz_ptr} = getelementptr {frame_ty}, ptr null, i32 1"
                );
                let _ = writeln!(self.out, "  {sz_i64} = ptrtoint ptr {sz_ptr} to i64");
                let _ = writeln!(self.out, "  {raw} = call ptr @calloc(i64 1, i64 {sz_i64})");
                let _ = writeln!(self.out, "  {is_oom} = icmp eq ptr {raw}, null");
                let _ = writeln!(
                    self.out,
                    "  br i1 {is_oom}, label %{l_oom}, label %{l_init}"
                );

                // OOM: drop untransferred env in place, store null handle.
                let _ = writeln!(self.out, "{l_oom}:");
                if env_lt != "void" && drop_sym.is_some() {
                    let tmp_env = self.fresh();
                    let _ = writeln!(self.out, "  {tmp_env} = alloca {env_st}");
                    let _ = writeln!(self.out, "  store {env_lt} {env_val}, ptr {tmp_env}");
                    self.emit_drop_ptr(&tmp_env, env_ty)?;
                }
                let _ = writeln!(self.out, "  store ptr null, ptr {d_ptr}");
                let _ = writeln!(self.out, "  br label %{l_done}");

                // Initialize header + env, release-publish OpenIdle(0), then call C register_fn.
                let _ = writeln!(self.out, "{l_init}:");
                let gate_p = self.fresh();
                let rfn_p = self.fresh();
                let pfn_p = self.fresh();
                let dfn_p = self.fresh();
                let env_p = self.fresh();
                let tok = self.fresh();
                let tok_null = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {rfn_p} = getelementptr inbounds %__zil_areg_hdr, ptr {raw}, i32 0, i32 4"
                );
                let _ = writeln!(self.out, "  store ptr {req_fn}, ptr {rfn_p}");
                let _ = writeln!(
                    self.out,
                    "  {pfn_p} = getelementptr inbounds %__zil_areg_hdr, ptr {raw}, i32 0, i32 5"
                );
                let _ = writeln!(self.out, "  store ptr {drain_fn}, ptr {pfn_p}");
                let _ = writeln!(
                    self.out,
                    "  {dfn_p} = getelementptr inbounds %__zil_areg_hdr, ptr {raw}, i32 0, i32 6"
                );
                if let Some(dsym) = &drop_sym {
                    let _ = writeln!(self.out, "  store ptr @{dsym}, ptr {dfn_p}");
                } else {
                    let _ = writeln!(self.out, "  store ptr null, ptr {dfn_p}");
                }
                if env_lt != "void" {
                    let _ = writeln!(
                        self.out,
                        "  {env_p} = getelementptr inbounds {frame_ty}, ptr {raw}, i32 0, i32 1"
                    );
                    let _ = writeln!(self.out, "  store {env_lt} {env_val}, ptr {env_p}");
                }
                let _ = writeln!(
                    self.out,
                    "  {gate_p} = getelementptr inbounds %__zil_areg_hdr, ptr {raw}, i32 0, i32 0"
                );
                let _ = writeln!(
                    self.out,
                    "  store atomic i32 0, ptr {gate_p} release, align 4"
                );
                let _ = writeln!(
                    self.out,
                    "  {tok} = call ptr {reg_fn}(ptr @{tramp_sym}, ptr {raw})"
                );
                let _ = writeln!(self.out, "  {tok_null} = icmp eq ptr {tok}, null");
                let _ = writeln!(
                    self.out,
                    "  br i1 {tok_null}, label %{l_fail}, label %{l_ok}"
                );

                // Install failure: mark closing, drop env once, publish Closed(4), free frame.
                let _ = writeln!(self.out, "{l_fail}:");
                let _ = writeln!(self.out, "  call void @__zil_areg_mark_closing(ptr {raw})");
                if let Some(dsym) = &drop_sym {
                    let _ = writeln!(self.out, "  store ptr null, ptr {dfn_p}");
                    let _ = writeln!(self.out, "  call void @{dsym}(ptr {raw})");
                }
                let _ = writeln!(
                    self.out,
                    "  store atomic i32 4, ptr {gate_p} release, align 4"
                );
                let _ = writeln!(self.out, "  call void @free(ptr {raw})");
                let _ = writeln!(self.out, "  store ptr null, ptr {d_ptr}");
                let _ = writeln!(self.out, "  br label %{l_done}");

                // Success: publish token and return frame handle.
                let _ = writeln!(self.out, "{l_ok}:");
                let tok_p = self.fresh();
                let tok_i64 = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {tok_p} = getelementptr inbounds %__zil_areg_hdr, ptr {raw}, i32 0, i32 3"
                );
                let _ = writeln!(self.out, "  {tok_i64} = ptrtoint ptr {tok} to i64");
                let _ = writeln!(
                    self.out,
                    "  store atomic i64 {tok_i64}, ptr {tok_p} release, align 8"
                );
                let _ = writeln!(self.out, "  store ptr {raw}, ptr {d_ptr}");
                let _ = writeln!(self.out, "  br label %{l_done}");

                let _ = writeln!(self.out, "{l_done}:");
                Ok(())
            }
            IntrinsicOp::AsyncRegistrationValid => {
                let (frame, _, _) = self.emit_operand(&args[0])?;
                let ok = self.fresh();
                let _ = writeln!(self.out, "  {ok} = icmp ne ptr {frame}, null");
                let _ = writeln!(self.out, "  store i1 {ok}, ptr {d_ptr}");
                Ok(())
            }
            IntrinsicOp::AsyncRegistrationRequestClose => {
                let (reg_ref, _, _) = self.emit_operand(&args[0])?;
                let frame = self.fresh();
                let _ = writeln!(self.out, "  {frame} = load ptr, ptr {reg_ref}");
                let _ = writeln!(
                    self.out,
                    "  call void @__zil_areg_request_close(ptr {frame})"
                );
                Ok(())
            }
            IntrinsicOp::AsyncRegistrationPollClose => {
                let (reg_ref, _, _) = self.emit_operand(&args[0])?;
                let frame = self.fresh();
                let ready = self.fresh();
                let tag = self.fresh();
                let _ = writeln!(self.out, "  {frame} = load ptr, ptr {reg_ref}");
                let _ = writeln!(
                    self.out,
                    "  {ready} = call i1 @__zil_areg_poll_close(ptr {frame})"
                );
                // Poll.Pending -> tag 0, Poll.Ready(()) -> tag 1
                let _ = writeln!(self.out, "  {tag} = zext i1 {ready} to i32");
                let _ = writeln!(self.out, "  store i32 {tag}, ptr {d_ptr}");
                Ok(())
            }
            IntrinsicOp::AsyncRegistrationFinishDestroy => {
                let (frame, _, _) = self.emit_operand(&args[0])?;
                let _ = writeln!(
                    self.out,
                    "  call void @__zil_areg_finish_destroy(ptr {frame})"
                );
                Ok(())
            }
            IntrinsicOp::ScopedCallback => {
                if args.len() != 4 {
                    return Err(BodyEmitError::new(
                        "ScopedCallback expects 4 arguments (reg, close, &mut f, body)",
                    ));
                }
                let (reg_fn, _, _) = self.emit_operand(&args[0])?;
                let (close_fn, _, _) = self.emit_operand(&args[1])?;
                let (f_ptr, _, f_ref_ty_opt) = self.emit_operand(&args[2])?;
                let f_ref_ty = f_ref_ty_opt.ok_or_else(|| {
                    BodyEmitError::new("ScopedCallback missing callback ref type")
                })?;
                let Some(TyKind::Ref { pointee: f_ty, .. }) = self.tys().get(f_ref_ty) else {
                    return Err(BodyEmitError::new("ScopedCallback arg 2 must be &mut F"));
                };
                let f_ty = *f_ty;
                let (body_ptr, body_ty) = match &args[3] {
                    Operand::Copy(p) | Operand::Move(p) => self.emit_place_ptr(p)?,
                    Operand::Const(_) | Operand::Pooled(_) => {
                        return Err(BodyEmitError::new(
                            "ScopedCallback body must be a closure or fn ptr place",
                        ));
                    }
                };

                let tid = self.tmp;
                self.tmp += 1;
                let tramp_sym = format!("__zil_scoped_tramp_f{}_{tid}", self.fn_id.0);
                let f_closure_sym = match self.tys().get(f_ty) {
                    Some(TyKind::Closure { def, .. }) => {
                        let c_fn_id = self
                            .module
                            .functions
                            .iter()
                            .position(|f| f.body.closure == Some(*def))
                            .map(FnId::from_index)
                            .ok_or_else(|| {
                                BodyEmitError::new("ScopedCallback callback closure body missing")
                            })?;
                        Some(fn_symbol(c_fn_id, &self.module.functions[c_fn_id.index()]))
                    }
                    Some(TyKind::FnPtr { .. }) => None,
                    _ => {
                        return Err(BodyEmitError::new(
                            "ScopedCallback callback must be a closure or fn ptr",
                        ));
                    }
                };

                let _ = writeln!(
                    self.extra_defs,
                    "\ndefine internal i64 @{tramp_sym}(ptr %ctx, i32 %arg) {{"
                );
                self.extra_defs.push_str(
                    "entry:\n\
  %is_null = icmp eq ptr %ctx, null\n\
  br i1 %is_null, label %reject, label %check\n\
check:\n\
  %gate_p = getelementptr inbounds { i32, ptr }, ptr %ctx, i32 0, i32 0\n\
  %g = load i32, ptr %gate_p\n\
  %is_open = icmp eq i32 %g, 0\n\
  br i1 %is_open, label %run, label %reject\n\
run:\n\
  store i32 1, ptr %gate_p\n\
  %env_slot = getelementptr inbounds { i32, ptr }, ptr %ctx, i32 0, i32 1\n\
  %env_p = load ptr, ptr %env_slot\n",
                );
                if let Some(csym) = f_closure_sym {
                    let _ = writeln!(
                        self.extra_defs,
                        "  %rv = call i32 @{csym}(ptr %env_p, i32 %arg)"
                    );
                } else {
                    self.extra_defs
                        .push_str("  %f_val = load ptr, ptr %env_p\n");
                    self.extra_defs
                        .push_str("  %rv = call i32 %f_val(i32 %arg)\n");
                }
                self.extra_defs.push_str(
                    "  %g_after = load i32, ptr %gate_p\n\
  %still_busy = icmp eq i32 %g_after, 1\n\
  br i1 %still_busy, label %restore, label %ret_ok\n\
restore:\n\
  store i32 0, ptr %gate_p\n\
  br label %ret_ok\n\
ret_ok:\n\
  %rv64 = zext i32 %rv to i64\n\
  ret i64 %rv64\n\
reject:\n\
  ret i64 4294967296\n\
}\n",
                );

                let (body_closure_sym, body_is_fn_once) = match self.tys().get(body_ty) {
                    Some(TyKind::Closure { def, .. }) => {
                        let c_fn_id = self
                            .module
                            .functions
                            .iter()
                            .position(|f| f.body.closure == Some(*def))
                            .map(FnId::from_index)
                            .ok_or_else(|| {
                                BodyEmitError::new("ScopedCallback body closure missing")
                            })?;
                        let is_once = self
                            .tys()
                            .closure(*def)
                            .is_some_and(|c| c.kind == zil_ir::body::ClosureKind::FnOnce);
                        (
                            Some(fn_symbol(c_fn_id, &self.module.functions[c_fn_id.index()])),
                            is_once,
                        )
                    }
                    Some(TyKind::FnPtr { .. }) => (None, false),
                    _ => {
                        return Err(BodyEmitError::new(
                            "ScopedCallback body must be a closure or fn ptr",
                        ));
                    }
                };

                let frame = self.fresh();
                let gate_p = self.fresh();
                let env_slot = self.fresh();
                let tok = self.fresh();
                let is_null = self.fresh();
                let l_null = format!("sc_null_{tid}");
                let l_ok = format!("sc_ok_{tid}");
                let l_done = format!("sc_done_{tid}");
                let _ = writeln!(self.out, "  {frame} = alloca {{ i32, ptr }}");
                let _ = writeln!(
                    self.out,
                    "  {gate_p} = getelementptr inbounds {{ i32, ptr }}, ptr {frame}, i32 0, i32 0"
                );
                let _ = writeln!(self.out, "  store i32 0, ptr {gate_p}");
                let _ = writeln!(
                    self.out,
                    "  {env_slot} = getelementptr inbounds {{ i32, ptr }}, ptr {frame}, i32 0, i32 1"
                );
                let _ = writeln!(self.out, "  store ptr {f_ptr}, ptr {env_slot}");
                let _ = writeln!(
                    self.out,
                    "  {tok} = call ptr {reg_fn}(ptr @{tramp_sym}, ptr {frame})"
                );
                let _ = writeln!(self.out, "  {is_null} = icmp eq ptr {tok}, null");
                let _ = writeln!(
                    self.out,
                    "  br i1 {is_null}, label %{l_null}, label %{l_ok}"
                );

                // Registration returned NULL: skip body, deterministically drop body environment, return (false, 0).
                let _ = writeln!(self.out, "{l_null}:");
                self.emit_drop_ptr(&body_ptr, body_ty)?;
                let p0_n = self.fresh();
                let p1_n = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {p0_n} = getelementptr inbounds {{ i1, i32 }}, ptr {d_ptr}, i32 0, i32 0"
                );
                let _ = writeln!(self.out, "  store i1 false, ptr {p0_n}");
                let _ = writeln!(
                    self.out,
                    "  {p1_n} = getelementptr inbounds {{ i1, i32 }}, ptr {d_ptr}, i32 0, i32 1"
                );
                let _ = writeln!(self.out, "  store i32 0, ptr {p1_n}");
                let _ = writeln!(self.out, "  br label %{l_done}");

                // Registration succeeded: run body, clean up body before close, mark gate closed, call close, return (true, bres).
                let _ = writeln!(self.out, "{l_ok}:");
                let bres = self.fresh();
                if let Some(bsym) = body_closure_sym {
                    let _ = writeln!(self.out, "  {bres} = call i32 @{bsym}(ptr {body_ptr})");
                } else {
                    let bfp = self.fresh();
                    let _ = writeln!(self.out, "  {bfp} = load ptr, ptr {body_ptr}");
                    let _ = writeln!(self.out, "  {bres} = call i32 {bfp}()");
                }
                if !body_is_fn_once {
                    self.emit_drop_ptr(&body_ptr, body_ty)?;
                }
                let _ = writeln!(self.out, "  store i32 2, ptr {gate_p}");
                let _ = writeln!(self.out, "  call void {close_fn}(ptr {tok})");
                let p0_ok = self.fresh();
                let p1_ok = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {p0_ok} = getelementptr inbounds {{ i1, i32 }}, ptr {d_ptr}, i32 0, i32 0"
                );
                let _ = writeln!(self.out, "  store i1 true, ptr {p0_ok}");
                let _ = writeln!(
                    self.out,
                    "  {p1_ok} = getelementptr inbounds {{ i1, i32 }}, ptr {d_ptr}, i32 0, i32 1"
                );
                let _ = writeln!(self.out, "  store i32 {bres}, ptr {p1_ok}");
                let _ = writeln!(self.out, "  br label %{l_done}");

                let _ = writeln!(self.out, "{l_done}:");
                Ok(())
            }
        }
    }

    fn store_elem_ty(&self, handle_ty: Option<TyId>) -> Result<TyId, BodyEmitError> {
        let ty = handle_ty.ok_or_else(|| BodyEmitError::new("Store handle missing type"))?;
        let Some(TyKind::Desc(inner) | TyKind::Managed(inner)) = self.tys().get(ty) else {
            return Err(BodyEmitError::new("Store handle is not Desc/Managed"));
        };
        if let Some(TyKind::Opaque(OpaqueTy::OwnedStore { element: elem })) = self.tys().get(*inner)
        {
            Ok(*elem)
        } else {
            Err(BodyEmitError::new("Store handle inner is not OwnedStore"))
        }
    }
}

fn emit_function_decl(
    module: &Module,
    func: &Function,
    sym: &str,
    out: &mut String,
) -> Result<(), BodyEmitError> {
    let tys = &module.tys;
    let ret = llvm_ty(tys, func.sig.result)?;
    let mut param_tys = Vec::new();
    if func.body.closure.is_some() {
        param_tys.push("ptr".to_string());
    }
    for &pt in &func.sig.params {
        let lt = llvm_ty(tys, pt)?;
        if lt != "void" {
            param_tys.push(lt);
        }
    }
    let _ = writeln!(out, "declare {ret} @{sym}({})", param_tys.join(", "));
    Ok(())
}

#[derive(Clone, Copy)]
struct FnEmitConfig {
    is_windows: bool,
    internalize: bool,
    emit_cancel_clear: bool,
}

fn emit_function(
    module: &Module,
    fn_id: FnId,
    func: &Function,
    sym: &str,
    cfg: FnEmitConfig,
    out: &mut String,
) -> Result<(), BodyEmitError> {
    let is_windows = cfg.is_windows;
    let internalize = cfg.internalize;
    let emit_cancel_clear = cfg.emit_cancel_clear;
    let tys = &module.tys;
    let ret = llvm_ty(tys, func.sig.result)?;
    let mut param_sigs = Vec::new();
    if func.body.closure.is_some() {
        param_sigs.push("ptr %__env".to_string());
    }
    for (i, &pt) in func.sig.params.iter().enumerate() {
        let lt = llvm_ty(tys, pt)?;
        if lt != "void" {
            param_sigs.push(format!("{lt} %p{i}"));
        }
    }
    let linkage = if internalize { "internal " } else { "" };
    let _ = writeln!(
        out,
        "define {linkage}{ret} @{sym}({}) {{",
        param_sigs.join(", ")
    );
    out.push_str("entry:\n");
    for (i, local) in func.body.locals.iter().enumerate() {
        let st = llvm_storage_ty(tys, local.ty)?;
        let _ = writeln!(out, "  %l{i} = alloca {st}");
    }
    for (i, &pt) in func.sig.params.iter().enumerate() {
        let lt = llvm_ty(tys, pt)?;
        if lt != "void" {
            let _ = writeln!(out, "  store {lt} %p{i}, ptr %l{i}");
        }
    }
    let mut entry_allocas = String::new();
    let mut body_out = String::new();
    let extra = {
        let mut emitter = FnEmitter {
            module,
            fn_id,
            body: &func.body,
            is_windows,
            emit_cancel_clear,
            out: &mut body_out,
            entry_allocas: Some(&mut entry_allocas),
            extra_defs: String::new(),
            tmp: 0,
        };
        for (bi, block) in func.body.blocks.iter().enumerate() {
            emit_block(&mut emitter, bi, block)?;
        }
        emitter.extra_defs
    };
    out.push_str(&entry_allocas);
    out.push_str("  br label %bb0\n");
    out.push_str(&body_out);
    out.push_str("}\n");
    out.push_str(&extra);
    Ok(())
}

fn emit_block(emitter: &mut FnEmitter<'_>, bi: usize, block: &Block) -> Result<(), BodyEmitError> {
    let _ = writeln!(emitter.out, "bb{bi}:");
    for inst in &block.instructions {
        emitter.emit_instruction(&inst.kind)?;
    }
    emitter.emit_terminator(&block.terminator)
}

fn emit_dyn_vtables(
    module: &Module,
    syms: &[String],
    is_windows: bool,
    internalize: bool,
    reachable_dyn_impls: &[bool],
    out: &mut String,
) -> Result<(), BodyEmitError> {
    let empty_body = Body {
        locals: Vec::new(),
        blocks: Vec::new(),
        param_count: 0,
        result: TyId(0),
        captures: Vec::new(),
        closure: None,
        drop_flags: Vec::new(),
        param_env: zil_ir::body::ParamEnv::default(),
    };
    let vtable_linkage = if internalize { "internal " } else { "" };
    for (idx, dimpl) in module.dyn_impls.iter().enumerate() {
        if !reachable_dyn_impls.get(idx).copied().unwrap_or(true) {
            continue;
        }
        let def = module
            .tys
            .get_dyn(dimpl.dyn_id)
            .ok_or_else(|| BodyEmitError::new(format!("missing DynDef {:?}", dimpl.dyn_id)))?;
        // 1. Drop shim: destruct concrete value and free heap allocation.
        let drop_sym = format!("__zil_dyn_drop_{idx}");
        let _ = writeln!(out, "define internal void @{drop_sym}(ptr %data) {{");
        let _ = writeln!(out, "entry:");
        let extra_defs = {
            let mut emitter = FnEmitter {
                module,
                fn_id: FnId(0),
                body: &empty_body,
                is_windows,
                emit_cancel_clear: false,
                out,
                entry_allocas: None,
                extra_defs: String::new(),
                tmp: 0,
            };
            emitter.emit_drop_ptr("%data", dimpl.concrete_ty)?;
            emitter.extra_defs
        };
        let _ = writeln!(out, "  call void @free(ptr %data)");
        let _ = writeln!(out, "  ret void");
        let _ = writeln!(out, "}}\n");
        if !extra_defs.is_empty() {
            out.push_str(&extra_defs);
        }

        // 2. Method slot thunks.
        let mut entries = Vec::with_capacity(1 + dimpl.methods.len());
        entries.push(format!("ptr @{drop_sym}"));
        let concrete_llvm = llvm_ty(&module.tys, dimpl.concrete_ty)?;
        for (slot_idx, (slot, &fn_id)) in def.slots.iter().zip(&dimpl.methods).enumerate() {
            let thunk_sym = format!("__zil_dyn_slot_{idx}_{slot_idx}");
            let target_sym = &syms[fn_id.index()];
            let ret_llvm = llvm_ty(&module.tys, slot.result)?;
            let mut thunk_params = vec!["ptr %data".to_string()];
            let mut call_args = Vec::with_capacity(1 + slot.params.len());
            let mut body_prefix = String::new();
            if slot.receiver.is_some() {
                call_args.push("ptr %data".to_string());
            } else {
                if concrete_llvm != "void" {
                    let _ = writeln!(body_prefix, "  %self_val = load {concrete_llvm}, ptr %data");
                    call_args.push(format!("{concrete_llvm} %self_val"));
                }
                let _ = writeln!(body_prefix, "  call void @free(ptr %data)");
            }
            for (pi, &pty) in slot.params.iter().enumerate() {
                let plt = llvm_ty(&module.tys, pty)?;
                if plt != "void" {
                    thunk_params.push(format!("{plt} %a{pi}"));
                    call_args.push(format!("{plt} %a{pi}"));
                }
            }
            let _ = writeln!(
                out,
                "define internal {ret_llvm} @{thunk_sym}({}) {{",
                thunk_params.join(", ")
            );
            let _ = writeln!(out, "entry:");
            out.push_str(&body_prefix);
            if ret_llvm == "void" {
                let _ = writeln!(out, "  call void @{target_sym}({})", call_args.join(", "));
                let _ = writeln!(out, "  ret void");
            } else {
                let _ = writeln!(
                    out,
                    "  %r = call {ret_llvm} @{target_sym}({})",
                    call_args.join(", ")
                );
                let _ = writeln!(out, "  ret {ret_llvm} %r");
            }
            let _ = writeln!(out, "}}\n");
            entries.push(format!("ptr @{thunk_sym}"));
        }

        // 3. Static constant vtable global.
        let n = entries.len();
        let _ = writeln!(
            out,
            "@__zil_vtable_{idx} = {vtable_linkage}constant [{n} x ptr] [{}]\n",
            entries.join(", ")
        );
    }
    Ok(())
}

struct CodegenReachability {
    fns: Vec<bool>,
    dyn_impls: Vec<bool>,
    uses_tasks: bool,
    uses_regs: bool,
    uses_aregs: bool,
    uses_bytes_grow: bool,
}

impl CodegenReachability {
    fn all(module: &Module) -> Self {
        let mut uses_bytes_grow = false;
        for f in &module.functions {
            for b in &f.body.blocks {
                if let Terminator::Call {
                    callee: Callee::Intrinsic(IntrinsicOp::ByteBufferPush),
                    ..
                } = &b.terminator
                {
                    uses_bytes_grow = true;
                }
            }
        }
        Self {
            fns: vec![true; module.functions.len()],
            dyn_impls: vec![true; module.dyn_impls.len()],
            uses_tasks: true,
            uses_regs: true,
            uses_aregs: true,
            uses_bytes_grow,
        }
    }
}

fn compute_codegen_reachability(module: &Module, root: Option<FnId>) -> CodegenReachability {
    let Some(root) = root else {
        return CodegenReachability::all(module);
    };
    let mut closure_to_fn = std::collections::HashMap::new();
    for (idx, f) in module.functions.iter().enumerate() {
        if let Some(cid) = f.body.closure {
            closure_to_fn.insert(cid, FnId::from_index(idx));
        }
    }

    let mut reach = CodegenReachability {
        fns: vec![false; module.functions.len()],
        dyn_impls: vec![false; module.dyn_impls.len()],
        uses_tasks: false,
        uses_regs: false,
        uses_aregs: false,
        uses_bytes_grow: false,
    };
    let mut visited_tys = std::collections::HashSet::<TyId>::new();
    let mut fn_queue = std::collections::VecDeque::<FnId>::new();
    let mut ty_queue = std::collections::VecDeque::<TyId>::new();

    let enqueue_fn =
        |fid: FnId, reach: &mut CodegenReachability, q: &mut std::collections::VecDeque<FnId>| {
            if let Some(slot) = reach.fns.get_mut(fid.index())
                && !*slot
            {
                *slot = true;
                q.push_back(fid);
            }
        };
    let enqueue_ty = |tid: TyId,
                      visited: &mut std::collections::HashSet<TyId>,
                      q: &mut std::collections::VecDeque<TyId>| {
        if visited.insert(tid) {
            q.push_back(tid);
        }
    };

    enqueue_fn(root, &mut reach, &mut fn_queue);

    while !fn_queue.is_empty() || !ty_queue.is_empty() {
        while let Some(fid) = fn_queue.pop_front() {
            let func = &module.functions[fid.index()];
            enqueue_ty(func.sig.result, &mut visited_tys, &mut ty_queue);
            for &pt in &func.sig.params {
                enqueue_ty(pt, &mut visited_tys, &mut ty_queue);
            }
            for loc in &func.body.locals {
                enqueue_ty(loc.ty, &mut visited_tys, &mut ty_queue);
            }
            for &cap in &func.body.captures {
                enqueue_ty(cap, &mut visited_tys, &mut ty_queue);
            }
            for block in &func.body.blocks {
                for inst in &block.instructions {
                    match &inst.kind {
                        InstructionKind::Assign { value, .. } => match value {
                            Rvalue::FnRef(target) => enqueue_fn(*target, &mut reach, &mut fn_queue),
                            Rvalue::ErrorUnion { ty, .. } => {
                                enqueue_ty(*ty, &mut visited_tys, &mut ty_queue);
                            }
                            Rvalue::Convert { to, .. } => {
                                enqueue_ty(*to, &mut visited_tys, &mut ty_queue);
                            }
                            Rvalue::Aggregate { kind, .. } => match kind {
                                AggregateKind::Closure { def, .. } => {
                                    if let Some(&cfn) = closure_to_fn.get(def) {
                                        enqueue_fn(cfn, &mut reach, &mut fn_queue);
                                    }
                                }
                                AggregateKind::Adt { adt, args, .. } => {
                                    if let Some(drop_fn) = module.drop_fn(*adt) {
                                        enqueue_fn(drop_fn, &mut reach, &mut fn_queue);
                                    }
                                    if let Some(ga) = module.tys.get_args(*args) {
                                        for &t in &ga.tys {
                                            enqueue_ty(t, &mut visited_tys, &mut ty_queue);
                                        }
                                    }
                                }
                                AggregateKind::Array { element } => {
                                    enqueue_ty(*element, &mut visited_tys, &mut ty_queue);
                                }
                                AggregateKind::Tuple => {}
                            },
                            Rvalue::Use(_)
                            | Rvalue::Ref { .. }
                            | Rvalue::SliceRef { .. }
                            | Rvalue::Describe { .. }
                            | Rvalue::Unary { .. }
                            | Rvalue::Binary { .. }
                            | Rvalue::Discriminant(_)
                            | Rvalue::ErrorArm(_)
                            | Rvalue::Len(_)
                            | Rvalue::ForeignFnRef(_)
                            | Rvalue::StaticUtf(_) => {}
                        },
                        InstructionKind::Drop { .. }
                        | InstructionKind::Replace { .. }
                        | InstructionKind::StorageLive(_)
                        | InstructionKind::StorageDead(_)
                        | InstructionKind::ScopeEnter(_)
                        | InstructionKind::ScopeExit { .. }
                        | InstructionKind::RegisterDefer { .. } => {}
                    }
                }
                match &block.terminator {
                    Terminator::Call { callee, .. } => match callee {
                        Callee::Direct(target) => enqueue_fn(*target, &mut reach, &mut fn_queue),
                        Callee::Intrinsic(op) => match op {
                            IntrinsicOp::ByteBufferPush => {
                                reach.uses_bytes_grow = true;
                            }
                            IntrinsicOp::TaskStart
                            | IntrinsicOp::TaskCreate
                            | IntrinsicOp::TaskYield
                            | IntrinsicOp::TaskValid
                            | IntrinsicOp::TaskPoll
                            | IntrinsicOp::TaskRequestCancel
                            | IntrinsicOp::TaskPollClose
                            | IntrinsicOp::TaskFinishDestroy => {
                                reach.uses_tasks = true;
                            }
                            IntrinsicOp::RegistrationStart
                            | IntrinsicOp::RegistrationValid
                            | IntrinsicOp::RegistrationClose => {
                                reach.uses_regs = true;
                            }
                            IntrinsicOp::AsyncRegistrationStart
                            | IntrinsicOp::AsyncRegistrationValid
                            | IntrinsicOp::AsyncRegistrationRequestClose
                            | IntrinsicOp::AsyncRegistrationPollClose
                            | IntrinsicOp::AsyncRegistrationFinishDestroy => {
                                reach.uses_aregs = true;
                            }
                            IntrinsicOp::StoreNew
                            | IntrinsicOp::StoreReserve
                            | IntrinsicOp::StorePush
                            | IntrinsicOp::StorePop
                            | IntrinsicOp::StoreLen
                            | IntrinsicOp::StoreCapacity
                            | IntrinsicOp::StoreBorrow { .. }
                            | IntrinsicOp::ByteBufferNew
                            | IntrinsicOp::ByteBufferReserve
                            | IntrinsicOp::ByteBufferTruncate
                            | IntrinsicOp::ByteBufferLen
                            | IntrinsicOp::ByteBufferCapacity
                            | IntrinsicOp::ByteBufferBorrow
                            | IntrinsicOp::MutexCreate
                            | IntrinsicOp::MutexLock
                            | IntrinsicOp::MutexValue
                            | IntrinsicOp::MutexUnlock
                            | IntrinsicOp::SyncLoad
                            | IntrinsicOp::SyncStore
                            | IntrinsicOp::SyncFetchAdd
                            | IntrinsicOp::SyncCompareExchange
                            | IntrinsicOp::ManagedCreate
                            | IntrinsicOp::OwnedDynCreate
                            | IntrinsicOp::ThreadScope
                            | IntrinsicOp::SyncInit
                            | IntrinsicOp::ScopedCallback
                            | IntrinsicOp::SyncLoadOrdered(_)
                            | IntrinsicOp::SyncStoreOrdered(_)
                            | IntrinsicOp::SyncFetchAddOrdered(_)
                            | IntrinsicOp::SyncExchangeOrdered(_)
                            | IntrinsicOp::SyncCompareExchangeOrdered { .. }
                            | IntrinsicOp::CNull
                            | IntrinsicOp::CCallback => {}
                        },
                        Callee::Indirect(_)
                        | Callee::Closure { .. }
                        | Callee::DynSlot { .. }
                        | Callee::Foreign(_) => {}
                    },
                    Terminator::Goto { .. }
                    | Terminator::Switch { .. }
                    | Terminator::Return { .. }
                    | Terminator::Assert { .. }
                    | Terminator::Unreachable
                    | Terminator::Trap(_) => {}
                }
            }
        }

        while let Some(tid) = ty_queue.pop_front() {
            let Some(kind) = module.tys.get(tid) else {
                continue;
            };
            match kind {
                TyKind::Adt { adt, args } => {
                    if let Some(drop_fn) = module.drop_fn(*adt) {
                        enqueue_fn(drop_fn, &mut reach, &mut fn_queue);
                    }
                    if let Some(ga) = module.tys.get_args(*args) {
                        for &t in &ga.tys {
                            enqueue_ty(t, &mut visited_tys, &mut ty_queue);
                        }
                    }
                    if let Some(def) = module.tys.get_adt(*adt) {
                        for v in &def.variants {
                            for f in &v.fields {
                                enqueue_ty(f.ty, &mut visited_tys, &mut ty_queue);
                            }
                        }
                    }
                }
                TyKind::Closure { def, args } => {
                    if let Some(&cfn) = closure_to_fn.get(def) {
                        enqueue_fn(cfn, &mut reach, &mut fn_queue);
                    }
                    if let Some(ga) = module.tys.get_args(*args) {
                        for &t in &ga.tys {
                            enqueue_ty(t, &mut visited_tys, &mut ty_queue);
                        }
                    }
                }
                TyKind::Dyn { def, .. } | TyKind::OwnedDyn { def, .. } => {
                    if let TyKind::OwnedDyn {
                        context: Some(ctx), ..
                    } = kind
                    {
                        enqueue_ty(*ctx, &mut visited_tys, &mut ty_queue);
                    }
                    for (idx, dimpl) in module.dyn_impls.iter().enumerate() {
                        if dimpl.dyn_id == *def && !reach.dyn_impls[idx] {
                            reach.dyn_impls[idx] = true;
                            enqueue_ty(dimpl.concrete_ty, &mut visited_tys, &mut ty_queue);
                            for &mfn in &dimpl.methods {
                                enqueue_fn(mfn, &mut reach, &mut fn_queue);
                            }
                        }
                    }
                }
                TyKind::Tuple(elems) => {
                    for &t in elems {
                        enqueue_ty(t, &mut visited_tys, &mut ty_queue);
                    }
                }
                TyKind::Array { element, .. }
                | TyKind::Slice(element)
                | TyKind::Ref {
                    pointee: element, ..
                }
                | TyKind::Managed(element)
                | TyKind::Desc(element) => {
                    enqueue_ty(*element, &mut visited_tys, &mut ty_queue);
                }
                TyKind::ErrorUnion { ok, err } => {
                    enqueue_ty(*ok, &mut visited_tys, &mut ty_queue);
                    enqueue_ty(*err, &mut visited_tys, &mut ty_queue);
                }
                TyKind::FnPtr { params, result } => {
                    for &p in params {
                        enqueue_ty(p, &mut visited_tys, &mut ty_queue);
                    }
                    enqueue_ty(*result, &mut visited_tys, &mut ty_queue);
                }
                TyKind::Opaque(op) => match op {
                    OpaqueTy::Task { result, context } => {
                        reach.uses_tasks = true;
                        enqueue_ty(*result, &mut visited_tys, &mut ty_queue);
                        if let Some(ctx) = context {
                            enqueue_ty(*ctx, &mut visited_tys, &mut ty_queue);
                        }
                    }
                    OpaqueTy::Registration { environment } => {
                        reach.uses_regs = true;
                        enqueue_ty(*environment, &mut visited_tys, &mut ty_queue);
                    }
                    OpaqueTy::AsyncRegistration { environment } => {
                        reach.uses_aregs = true;
                        enqueue_ty(*environment, &mut visited_tys, &mut ty_queue);
                    }
                    OpaqueTy::OwnedStore { element: elem }
                    | OpaqueTy::Mutex { payload: elem }
                    | OpaqueTy::MutexGuard { payload: elem, .. } => {
                        enqueue_ty(*elem, &mut visited_tys, &mut ty_queue);
                    }
                    OpaqueTy::ForeignCallback { params, result } => {
                        for &p in params {
                            enqueue_ty(p, &mut visited_tys, &mut ty_queue);
                        }
                        enqueue_ty(*result, &mut visited_tys, &mut ty_queue);
                    }
                    OpaqueTy::ByteBuffer
                    | OpaqueTy::ForeignHandle
                    | OpaqueTy::CWideString
                    | OpaqueTy::SyncCell(_) => {}
                },
                TyKind::Unit
                | TyKind::Bool
                | TyKind::Never
                | TyKind::Int(_)
                | TyKind::Float(_)
                | TyKind::Param(_) => {}
            }
        }
    }

    reach
}
