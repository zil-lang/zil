//! Function-level contract orchestration: `resume(transfer)`, CFG provenance, and state contracts.

use zil_ir::body::{FnId, Module, TyKind};

use crate::provenance::verify_fn_provenance;
use crate::state_protocol::verify_fn_state_contracts;
use crate::{CheckBudget, CheckError, CheckErrorKind};

/// Independently verifies function `resume(transfer)` contracts (including
/// transitive `Suspend` live-borrow freedom), fail-closed FFI `c_register_async`
/// provider role identity, and per-path/per-result protocol-state contracts
/// (`requires(state(...))`, `provides(retired(...))`, `on_pending`/`on_ready`).
pub(crate) fn check_contracts(
    module: &Module,
    can_suspend: &[bool],
    budget: CheckBudget,
) -> Result<(), CheckError> {
    use std::collections::{HashMap, VecDeque};
    use zil_ir::body::{
        BlockId, Callee, ClosureId, IntrinsicOp, OpLocation, Place, ResumeMode, Terminator,
    };

    let tys = &module.tys;
    let fail = |f: FnId, reason: &'static str| CheckError {
        function: f,
        kind: CheckErrorKind::CertificateShape(reason),
    };

    let mut closure_to_fn: HashMap<ClosureId, FnId> = HashMap::new();
    for (i, func) in module.functions.iter().enumerate() {
        if let Some(cid) = func.body.closure {
            closure_to_fn.insert(cid, FnId::from_index(i));
        }
    }

    let closure_fn_of_place = |body: &zil_ir::body::Body, place: &Place| -> Option<FnId> {
        let cx = body.cx(tys);
        let mut ty = zil_ir::body::type_place(place, &cx).ok()?.ty();
        for _ in 0..4 {
            match tys.get(ty)? {
                TyKind::Ref { pointee, .. } => ty = *pointee,
                TyKind::Closure { def, .. } => return closure_to_fn.get(def).copied(),
                TyKind::Unit
                | TyKind::Bool
                | TyKind::Int(_)
                | TyKind::Float(_)
                | TyKind::Never
                | TyKind::Tuple(_)
                | TyKind::Array { .. }
                | TyKind::Slice(_)
                | TyKind::Adt { .. }
                | TyKind::Managed(_)
                | TyKind::Desc(_)
                | TyKind::ErrorUnion { .. }
                | TyKind::FnPtr { .. }
                | TyKind::Dyn { .. }
                | TyKind::OwnedDyn { .. }
                | TyKind::Opaque(_)
                | TyKind::Param(_) => return None,
            }
        }
        None
    };

    // 1. Compute transitive set of functions reachable on a suspending path
    //    from any `ResumeMode::Transfer` function.
    let mut transfer_reachable = vec![false; module.functions.len()];
    let mut work: VecDeque<FnId> = VecDeque::new();
    for (fi, func) in module.functions.iter().enumerate() {
        if func.sig.resume_mode == ResumeMode::Transfer {
            transfer_reachable[fi] = true;
            work.push_back(FnId::from_index(fi));
        }
    }
    while let Some(fid) = work.pop_front() {
        let func = &module.functions[fid.index()];
        for block in &func.body.blocks {
            if let Terminator::Call { callee, .. } = &block.terminator {
                let target_fn = match callee {
                    Callee::Direct(cid) => Some(*cid),
                    Callee::Closure { closure, .. } => closure_fn_of_place(&func.body, closure),
                    Callee::Indirect(_)
                    | Callee::DynSlot { .. }
                    | Callee::Foreign(_)
                    | Callee::Intrinsic(_) => None,
                };
                if let Some(tid) = target_fn
                    && tid.index() < can_suspend.len()
                    && can_suspend[tid.index()]
                    && !transfer_reachable[tid.index()]
                {
                    transfer_reachable[tid.index()] = true;
                    work.push_back(tid);
                }
            }
        }
    }

    let check_one = |fi: usize| -> Result<(), CheckError> {
        let func = &module.functions[fi];
        let fn_id = FnId::from_index(fi);

        // 1a. Verify `resume(transfer)` functions (and every suspending helper
        //     transitively reached from them) hold no borrowed addresses in
        //     their ABI or live across any direct or indirect suspension point.
        if transfer_reachable[fi] {
            for &p in &func.sig.params {
                if tys.holds_address(p) || tys.may_store_address(p) {
                    return Err(fail(
                        fn_id,
                        "resume(transfer) function cannot accept borrowed addresses",
                    ));
                }
            }
            if func.sig.resume_mode == ResumeMode::Transfer
                && (tys.holds_address(func.sig.result) || tys.may_store_address(func.sig.result))
            {
                return Err(fail(
                    fn_id,
                    "resume(transfer) function cannot return borrowed addresses",
                ));
            }
            let live = zil_semantics::Liveness::compute(&func.body);
            for (bi, block) in func.body.blocks.iter().enumerate() {
                let suspends_here = match &block.terminator {
                    Terminator::Call {
                        callee: Callee::Intrinsic(IntrinsicOp::TaskYield),
                        ..
                    } => true,
                    Terminator::Call {
                        callee: Callee::Direct(cid),
                        ..
                    } => cid.index() < can_suspend.len() && can_suspend[cid.index()],
                    Terminator::Call {
                        callee: Callee::Closure { closure, .. },
                        ..
                    } => closure_fn_of_place(&func.body, closure).is_some_and(|cid| {
                        cid.index() < can_suspend.len() && can_suspend[cid.index()]
                    }),
                    Terminator::Call { .. }
                    | Terminator::Goto { .. }
                    | Terminator::Switch { .. }
                    | Terminator::Return { .. }
                    | Terminator::Assert { .. }
                    | Terminator::Unreachable
                    | Terminator::Trap(_) => false,
                };
                if suspends_here {
                    let at = OpLocation {
                        block: BlockId(bi as u32),
                        instruction: None,
                    };
                    for l_idx in live.after(at).iter() {
                        if let Some(decl) = func.body.locals.get(l_idx)
                            && tys.holds_address(decl.ty)
                        {
                            return Err(fail(
                                fn_id,
                                "resume(transfer) suspending function cannot hold borrowed addresses across yield",
                            ));
                        }
                    }
                }
            }
        }

        // 2 & 2b. Path-sensitive forward CFG fixed-point provenance verification:
        //         - `AsyncRegistrationStart` provider identity & roles (fail-closed in `transfer_reachable`);
        //         - `ScopedCallback` (`c_with_callback`) provider roles & no direct `Callee::Foreign` calls;
        //         - `ThreadScope` cross-thread `Managed<Task<...>>` transfer requires every reaching path
        //           to originate from a `TaskCreate` whose worker declares `ResumeMode::Transfer`.
        verify_fn_provenance(module, fn_id, func, transfer_reachable[fi])?;

        // 3. Verify declared protocol-state contracts (`requires(state(...))`,
        //    `requires(apart(...))`, `provides(state|retired|on_ok|on_err|on_pending|on_ready)`).
        //    Run on every function that declares state contracts OR calls any callee
        //    that declares state contracts, so unannotated callers cannot bypass preconditions.
        let calls_contracted_callee = func.body.blocks.iter().any(|b| {
            matches!(
                &b.terminator,
                Terminator::Call {
                    callee: zil_ir::body::Callee::Direct(cid),
                    ..
                } if module
                    .functions
                    .get(cid.index())
                    .is_some_and(|cf| !cf.sig.state_contracts.is_empty())
            )
        });
        if !func.sig.state_contracts.is_empty() || calls_contracted_callee {
            verify_fn_state_contracts(module, fn_id, func, budget)?;
        }
        Ok(())
    };

    let n = module.functions.len();
    let workers = std::thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(1)
        .clamp(1, 16)
        .min(n.max(1));
    if workers <= 1 || n < 16 {
        for fi in 0..n {
            check_one(fi)?;
        }
        return Ok(());
    }

    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    let next_idx = AtomicUsize::new(0);
    let any_failed = AtomicBool::new(false);
    let errors: Vec<Mutex<Option<CheckError>>> = (0..n).map(|_| Mutex::new(None)).collect();
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                loop {
                    if any_failed.load(Ordering::Relaxed) {
                        break;
                    }
                    let fi = next_idx.fetch_add(1, Ordering::Relaxed);
                    if fi >= n {
                        break;
                    }
                    if let Err(err) = check_one(fi) {
                        any_failed.store(true, Ordering::Relaxed);
                        if let Ok(mut guard) = errors[fi].lock() {
                            *guard = Some(err);
                        }
                    }
                }
            });
        }
    });
    if any_failed.load(Ordering::Relaxed) {
        // Re-check any skipped earlier function deterministically so the returned
        // error is always the lowest-index failing function.
        for (fi, slot) in errors.into_iter().enumerate() {
            if let Some(err) = slot.into_inner().ok().flatten() {
                return Err(err);
            }
            check_one(fi)?;
        }
    }
    Ok(())
}
