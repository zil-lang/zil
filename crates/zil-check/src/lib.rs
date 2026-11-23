//! Checker: the only authority. It never trusts a candidate conclusion; it
//! applies the semantic kernel once per operation and edge to the untrusted
//! certificate and discharges every obligation of
//! `spec/semantic-kernel.md` §6 that the implemented families cover.
#![forbid(unsafe_code)]
#![deny(clippy::wildcard_enum_match_arm)]

mod certificate;
pub mod ct_original;
pub mod fidelity;
pub mod scc;

pub use certificate::{CheckBudget, CheckError, CheckErrorKind, check_function};
pub use fidelity::{FidelityError, check_fidelity};
pub use scc::{CallScc, SccContractError, call_graph_sccs, validate_scc_contracts};
pub use zil_semantics::{Violation, ViolationKind};

use zil_ir::body::{AdtId, AdtKind, Certificate, FnId, FnRole, Module, Sigs, TyKind};

/// A module whose every function passed the checker. The only constructor
/// is [`check_module`]; the fields are private so no other crate can forge
/// one.
#[derive(Debug)]
pub struct VerifiedModule {
    module: Module,
}

impl VerifiedModule {
    pub fn module(&self) -> &Module {
        &self.module
    }
}

/// Checks `certificate` against `module` and grants admission.
pub fn check_module(
    module: Module,
    certificate: &Certificate,
    budget: CheckBudget,
) -> Result<VerifiedModule, CheckError> {
    check_drop_bindings(&module)?;
    check_dyn_impls(&module)?;
    let sccs = validate_scc_contracts(&module).map_err(|e| CheckError {
        function: e.function,
        kind: CheckErrorKind::CertificateShape(e.reason),
    })?;
    let can_suspend = check_suspend_effects(&module, &sccs)?;
    check_contracts(&module, &can_suspend, budget)?;
    let fn_sigs = module.fn_sigs();
    let return_summaries = zil_semantics::compute_return_summaries(&module);
    let sigs = Sigs {
        functions: &fn_sigs,
        foreigns: &module.foreigns,
        static_count: module.statics.len(),
        return_summaries: Some(&return_summaries),
    };
    let mut remaining = budget;
    if certificate.functions.len() != module.functions.len() {
        return Err(CheckError {
            function: FnId(0),
            kind: CheckErrorKind::CertificateShape("one function certificate per function"),
        });
    }
    for (i, cert) in certificate.functions.iter().enumerate() {
        let id = FnId::from_index(i);
        if cert.function != id {
            return Err(CheckError {
                function: id,
                kind: CheckErrorKind::CertificateShape("function certificates out of order"),
            });
        }
    }
    // Verify in reverse-topological SCC order (callees before callers, and
    // mutually recursive SCC members inductively against their declared contracts).
    for scc in &sccs {
        for &id in &scc.members {
            let function = &module.functions[id.index()];
            let cert = &certificate.functions[id.index()];
            check_function(&function.body, &module.tys, sigs, id, cert, &mut remaining)?;
        }
    }
    Ok(VerifiedModule { module })
}

/// `impl Drop` bindings (`_zil_design.md` §7): each cleanup entry belongs
/// to a resource declared with a user drop, takes exactly that resource's
/// descriptor and returns unit; every such resource has exactly one entry.
fn check_drop_bindings(module: &Module) -> Result<(), CheckError> {
    let tys = &module.tys;
    let fail = |i: usize, reason| CheckError {
        function: FnId::from_index(i),
        kind: CheckErrorKind::DropBinding(reason),
    };
    let mut bound: Vec<AdtId> = Vec::new();
    for (i, f) in module.functions.iter().enumerate() {
        let FnRole::Drop(adt) = f.sig.role else {
            continue;
        };
        let def = tys.get_adt(adt).ok_or(fail(i, "unknown resource"))?;
        if def.kind != AdtKind::Resource || !def.user_drop {
            return Err(fail(i, "cleanup entry of a type without a user drop"));
        }
        if def.type_params != 0 || def.region_params != 0 || f.sig.type_params != 0 {
            return Err(fail(i, "generic cleanup entry"));
        }
        let desc_of_adt = match f.sig.params.as_slice() {
            [p] => match tys.get(*p) {
                Some(TyKind::Desc(payload)) => {
                    matches!(tys.get(*payload), Some(TyKind::Adt { adt: a, .. }) if *a == adt)
                }
                Some(_) | None => false,
            },
            _ => false,
        };
        if !desc_of_adt || !matches!(tys.get(f.sig.result), Some(TyKind::Unit)) {
            return Err(fail(i, "cleanup entry signature is not fn(Desc<R>)"));
        }
        if bound.contains(&adt) {
            return Err(fail(i, "resource bound to two cleanup entries"));
        }
        bound.push(adt);
    }
    for a in 0..tys.adt_count() {
        let adt = AdtId::from_index(a);
        let def = tys.adt(adt);
        if def.kind == AdtKind::Resource && def.user_drop && !bound.contains(&adt) {
            return Err(fail(0, "resource with a user drop has no cleanup entry"));
        }
    }
    Ok(())
}

/// Verifies every concrete `dyn Trait` vtable implementation against its
/// declared `DynDef` slots, implementation identity (`FnRole::DynSlotImpl`),
/// and synchronous effect contract so a forged wire module cannot substitute
/// an unrelated same-signature `FnRole::Normal` function or unauthorized effect.
fn check_dyn_impls(module: &Module) -> Result<(), CheckError> {
    let tys = &module.tys;
    let fail = |f: FnId, reason: &'static str| CheckError {
        function: f,
        kind: CheckErrorKind::CertificateShape(reason),
    };
    let mut seen: Vec<(zil_ir::body::DynId, zil_ir::body::TyId)> = Vec::new();
    let mut bound_fns: Vec<FnId> = Vec::new();
    for dimpl in &module.dyn_impls {
        let def = tys
            .get_dyn(dimpl.dyn_id)
            .ok_or_else(|| fail(FnId(0), "dyn impl references unknown DynId"))?;
        if tys.get(dimpl.concrete_ty).is_none() {
            return Err(fail(FnId(0), "dyn impl references unknown concrete type"));
        }
        let key = (dimpl.dyn_id, dimpl.concrete_ty);
        if seen.contains(&key) {
            return Err(fail(FnId(0), "duplicate dyn impl for (DynId, concrete_ty)"));
        }
        seen.push(key);
        if dimpl.methods.len() != def.slots.len() {
            return Err(fail(FnId(0), "dyn impl method count does not match slots"));
        }
        for (slot_idx, (slot, &fn_id)) in def.slots.iter().zip(&dimpl.methods).enumerate() {
            let func = module
                .functions
                .get(fn_id.index())
                .ok_or_else(|| fail(fn_id, "dyn impl method FnId out of bounds"))?;
            let expected_role = FnRole::DynSlotImpl {
                dyn_id: dimpl.dyn_id,
                concrete_ty: dimpl.concrete_ty,
                slot: slot_idx as u32,
            };
            if func.sig.role != expected_role {
                return Err(fail(
                    fn_id,
                    "dyn impl method lacks matching FnRole::DynSlotImpl authorization",
                ));
            }
            if func.sig.type_params != 0 {
                return Err(fail(fn_id, "dyn impl method cannot be generic"));
            }
            if func.sig.params.len() != 1 + slot.params.len() {
                return Err(fail(fn_id, "dyn impl method arity mismatch"));
            }
            let recv_ty = func.sig.params[0];
            let recv_ok = match slot.receiver {
                Some(want_mut) => matches!(
                    tys.get(recv_ty),
                    Some(TyKind::Ref { mutability, pointee, .. })
                        if *mutability == want_mut
                            && tys.same_erased_region(*pointee, dimpl.concrete_ty)
                ),
                None => tys.same_erased_region(recv_ty, dimpl.concrete_ty),
            };
            if !recv_ok {
                return Err(fail(fn_id, "dyn impl method receiver type mismatch"));
            }
            if !func.sig.params[1..]
                .iter()
                .zip(&slot.params)
                .all(|(a, b)| tys.same_erased_region(*a, *b))
            {
                return Err(fail(fn_id, "dyn impl method parameter type mismatch"));
            }
            if !tys.same_erased_region(func.sig.result, slot.result) {
                return Err(fail(fn_id, "dyn impl method return type mismatch"));
            }
            if bound_fns.contains(&fn_id) {
                return Err(fail(fn_id, "dyn impl method bound to multiple slots"));
            }
            bound_fns.push(fn_id);
        }
    }
    // Every FnRole::DynSlotImpl function in the module must be uniquely bound by a DynImpl.
    for (i, func) in module.functions.iter().enumerate() {
        if matches!(func.sig.role, FnRole::DynSlotImpl { .. }) {
            let fid = FnId::from_index(i);
            if !bound_fns.contains(&fid) {
                return Err(fail(
                    fid,
                    "orphan FnRole::DynSlotImpl not referenced by DynImpl",
                ));
            }
        }
    }
    Ok(())
}

/// Independently verifies function `resume(transfer)` contracts (including
/// transitive `Suspend` live-borrow freedom), fail-closed FFI `c_register_async`
/// provider role identity, and per-path/per-result protocol-state contracts
/// (`requires(state(...))`, `provides(retired(...))`, `on_pending`/`on_ready`).
fn check_contracts(
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

    for (fi, func) in module.functions.iter().enumerate() {
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
    }
    Ok(())
}

/// Independently verifies built-in and user-defined `StateContractClause`s via
/// budget-bounded forward CFG abstract interpretation over finite state bitmasks,
/// `retired` flags, `ErrorUnion`/`Poll` exit refinements, and `apart(a, b)` aliasing.
fn verify_fn_state_contracts(
    module: &Module,
    fn_id: FnId,
    func: &zil_ir::body::Function,
    budget: CheckBudget,
) -> Result<(), CheckError> {
    use std::collections::{BTreeSet, HashMap, VecDeque};
    use zil_ir::body::{
        AdtId, AggregateKind, BlockId, Callee, ContractFieldPath, FieldIdx, InstructionKind,
        IntrinsicOp, Local, Mutability, OpaqueTy, Operand, Place, PlaceRoot, Projection,
        ProtocolStateSet, Rvalue, StateContractClause, Terminator,
    };

    let tys = &module.tys;
    let fail = |reason: &'static str| CheckError {
        function: fn_id,
        kind: CheckErrorKind::CertificateShape(reason),
    };

    #[derive(Clone, Copy, Debug)]
    struct ProtoHandleInfo {
        variant_count: usize,
        state_field: Option<(AdtId, FieldIdx)>,
    }

    let proto_handle_info = |mut ty: zil_ir::body::TyId| -> Option<ProtoHandleInfo> {
        for _ in 0..4 {
            match tys.get(ty) {
                Some(
                    TyKind::Ref { pointee, .. } | TyKind::Managed(pointee) | TyKind::Desc(pointee),
                ) => ty = *pointee,
                Some(TyKind::Opaque(
                    OpaqueTy::Task { .. }
                    | OpaqueTy::Registration { .. }
                    | OpaqueTy::AsyncRegistration { .. },
                )) => {
                    return Some(ProtoHandleInfo {
                        variant_count: 3,
                        state_field: None,
                    });
                }
                Some(TyKind::Adt { adt, .. }) => {
                    let def = tys.get_adt(*adt)?;
                    if def.kind == AdtKind::Enum
                        && !def.variants.is_empty()
                        && def.variants.len() <= 64
                    {
                        return Some(ProtoHandleInfo {
                            variant_count: def.variants.len(),
                            state_field: None,
                        });
                    }
                    if def.variants.len() == 1
                        && let Some((fi, f)) = def.variants[0]
                            .fields
                            .iter()
                            .enumerate()
                            .find(|(_, f)| f.name == "state")
                        && let Some(TyKind::Adt { adt: state_adt, .. }) = tys.get(f.ty)
                        && let Some(state_def) = tys.get_adt(*state_adt)
                        && state_def.kind == AdtKind::Enum
                        && !state_def.variants.is_empty()
                        && state_def.variants.len() <= 64
                    {
                        return Some(ProtoHandleInfo {
                            variant_count: state_def.variants.len(),
                            state_field: Some((*adt, FieldIdx(fi as u32))),
                        });
                    }
                    return None;
                }
                _ => return None,
            }
        }
        None
    };

    let is_poll_ty = |ty: zil_ir::body::TyId| -> bool {
        let inner = match tys.get(ty) {
            Some(TyKind::ErrorUnion { ok, .. }) => *ok,
            _ => ty,
        };
        match tys.get(inner) {
            Some(TyKind::Adt { adt, .. }) => tys
                .get_adt(*adt)
                .is_some_and(|d| d.kind == AdtKind::Enum && d.variants.len() == 2),
            _ => false,
        }
    };

    let is_error_union_ty =
        |ty: zil_ir::body::TyId| -> bool { matches!(tys.get(ty), Some(TyKind::ErrorUnion { .. })) };

    let is_mut_ref_or_desc_ty = |ty: zil_ir::body::TyId| -> bool {
        matches!(
            tys.get(ty),
            Some(
                TyKind::Ref {
                    mutability: Mutability::Exclusive,
                    ..
                } | TyKind::Desc(_)
            )
        )
    };

    let nlocals = func.body.locals.len();
    let param_count = func.body.param_count as usize;
    let local_info: Vec<Option<ProtoHandleInfo>> = func
        .body
        .locals
        .iter()
        .map(|l| proto_handle_info(l.ty))
        .collect();
    let mut init_states: Vec<ProtocolStateSet> = local_info
        .iter()
        .map(|info| {
            info.map_or(ProtocolStateSet(0), |i| {
                ProtocolStateSet::all_variants(i.variant_count)
            })
        })
        .collect();

    let mut has_poll_post = false;
    let mut has_eu_post = false;
    let mut has_invalidation_upper = false;
    let mut declared_invalidations: Vec<(usize, ContractFieldPath)> = Vec::new();
    let mut allows_rollback = vec![false; nlocals];
    let mut declared_requires_state: Vec<Option<ProtocolStateSet>> = vec![None; nlocals];
    for clause in &func.sig.state_contracts {
        match *clause {
            StateContractClause::InvalidationUpperBound => {
                has_invalidation_upper = true;
            }
            StateContractClause::Invalidates { param, path } => {
                has_invalidation_upper = true;
                let p_idx = param as usize;
                if p_idx >= param_count || p_idx >= func.sig.params.len() {
                    return Err(fail(
                        "invalidation contract references out-of-range parameter",
                    ));
                }
                if (path.len as usize) > ContractFieldPath::MAX_DEPTH {
                    return Err(fail("invalidation contract field path exceeds max depth"));
                }
                let mut cur_ty = func.sig.params[p_idx];
                for &fidx in path.as_slice() {
                    for _ in 0..4 {
                        if let Some(
                            TyKind::Ref { pointee, .. }
                            | TyKind::Managed(pointee)
                            | TyKind::Desc(pointee),
                        ) = tys.get(cur_ty)
                        {
                            cur_ty = *pointee;
                        } else {
                            break;
                        }
                    }
                    if let Some(TyKind::Adt { adt, .. }) = tys.get(cur_ty)
                        && let Some(def) = tys.get_adt(*adt)
                        && def.variants.len() == 1
                        && let Some(fdef) = def.variants[0].fields.get(fidx.0 as usize)
                    {
                        cur_ty = fdef.ty;
                    } else {
                        return Err(fail(
                            "invalidation contract field path does not match parameter ADT",
                        ));
                    }
                }
                declared_invalidations.push((p_idx, path));
            }
            StateContractClause::AllowsRollback { param } => {
                let p_idx = param as usize;
                if p_idx >= param_count || p_idx >= func.sig.params.len() {
                    return Err(fail("rollback contract references out-of-range parameter"));
                }
                if proto_handle_info(func.sig.params[p_idx]).is_none() {
                    return Err(fail(
                        "rollback contract parameter is not a protocol handle type",
                    ));
                }
                allows_rollback[p_idx] = true;
            }
            StateContractClause::RequiresApart { param_a, param_b } => {
                let pa = param_a as usize;
                let pb = param_b as usize;
                if pa == pb
                    || pa >= param_count
                    || pb >= param_count
                    || pa >= func.sig.params.len()
                    || pb >= func.sig.params.len()
                {
                    return Err(fail(
                        "state contract apart(...) references invalid parameter",
                    ));
                }
            }
            StateContractClause::RequiresState { param, states }
            | StateContractClause::ProvidesState { param, states }
            | StateContractClause::ProvidesOnPending { param, states }
            | StateContractClause::ProvidesOnReady { param, states }
            | StateContractClause::ProvidesOnOk { param, states }
            | StateContractClause::ProvidesOnErr { param, states } => {
                if matches!(
                    clause,
                    StateContractClause::ProvidesOnPending { .. }
                        | StateContractClause::ProvidesOnReady { .. }
                ) {
                    has_poll_post = true;
                }
                if matches!(
                    clause,
                    StateContractClause::ProvidesOnOk { .. }
                        | StateContractClause::ProvidesOnErr { .. }
                ) {
                    has_eu_post = true;
                }
                let p_idx = param as usize;
                if p_idx >= param_count || p_idx >= func.sig.params.len() {
                    return Err(fail("state contract references out-of-range parameter"));
                }
                let Some(info) = proto_handle_info(func.sig.params[p_idx]) else {
                    return Err(fail(
                        "state contract parameter is not a protocol handle type",
                    ));
                };
                if !states.is_valid_for(info.variant_count) {
                    return Err(fail("state contract has invalid ProtocolStateSet bitmask"));
                }
                if matches!(clause, StateContractClause::RequiresState { .. }) {
                    init_states[p_idx] = states;
                    declared_requires_state[p_idx] = Some(states);
                }
            }
            StateContractClause::ProvidesRetired { param } => {
                let p_idx = param as usize;
                if p_idx >= param_count || p_idx >= func.sig.params.len() {
                    return Err(fail("state contract references out-of-range parameter"));
                }
                if proto_handle_info(func.sig.params[p_idx]).is_none() {
                    return Err(fail(
                        "state contract parameter is not a protocol handle type",
                    ));
                }
            }
        }
    }
    if has_poll_post && !is_poll_ty(func.sig.result) {
        return Err(fail(
            "on_pending/on_ready state contract requires Poll return type",
        ));
    }
    if has_eu_post && !is_error_union_ty(func.sig.result) {
        return Err(fail(
            "on_ok/on_err state contract requires ErrorUnion return type",
        ));
    }

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum DiscrTarget {
        Poll(Local),
        ErrorUnion(Local),
        HandleState(usize),
        ValidCheck { handle: usize, inverted: bool },
    }

    type EuTransitions = Vec<(usize, Option<ProtocolStateSet>, Option<ProtocolStateSet>)>;
    type ParamAliasSet = BTreeSet<(usize, Vec<FieldIdx>)>;

    #[derive(Clone, PartialEq, Eq)]
    struct ProtoState {
        states: Vec<ProtocolStateSet>,
        /// History `H`: union of all protocol states visited by each handle on paths reaching here.
        history: Vec<ProtocolStateSet>,
        /// States that each handle has departed from on some path reaching here.
        departed_from: Vec<ProtocolStateSet>,
        /// True if any path reaching here re-entered a state it had already departed from (path-correlated rollback `H`).
        rolled_back: Vec<bool>,
        retired: Vec<bool>,
        /// Parameter projections `(param_idx, field_path)` invalidated on any path reaching here.
        invalidated: ParamAliasSet,
        /// May-alias map: local -> all `(param_idx, field_prefix)` it may alias on any reaching path (unioned on join).
        param_alias: HashMap<Local, ParamAliasSet>,
        /// Must-alias map: local -> unique protocol handle index (intersected on join; fail-closed when ambiguous).
        alias: HashMap<Local, usize>,
        /// May-alias map: local -> all protocol handle indices it may alias (unioned on join; conservatively widened on writes).
        may_alias: HashMap<Local, BTreeSet<usize>>,
        /// May-map: local (closure / dyn / aggregate wrapper) -> parameter projections reachable via captured `&mut`/`Desc`.
        mut_cap_params: HashMap<Local, ParamAliasSet>,
        /// May-map: local (closure / dyn / aggregate wrapper) -> protocol handle indices reachable via captured `&mut`/`Desc`.
        mut_cap_handles: HashMap<Local, BTreeSet<usize>>,
        /// Must-map: local holding copied `handle.state` field -> root handle index (intersected on join).
        state_field_of: HashMap<Local, usize>,
        /// Must-map: exact enum variant for locals constructed via `AggregateKind::Adt` (intersected on join).
        enum_variant: HashMap<Local, u32>,
        /// Must-map: exact ErrorUnion arm (`true = ok`, `false = err`) (intersected on join).
        error_arm_const: HashMap<Local, bool>,
        /// Must-map: Poll local -> `(handle_idx, pending_states, ready_states)` (intersected on join).
        poll_of: HashMap<Local, (usize, ProtocolStateSet, ProtocolStateSet)>,
        /// Must-map: ErrorUnion local -> `(handle_idx, ok_states, err_states)` transitions (intersected on join).
        eu_of: HashMap<Local, EuTransitions>,
        /// Must-map: discriminant/tag local -> target (intersected on join).
        discr_of: HashMap<Local, DiscrTarget>,
    }

    impl ProtoState {
        fn join(&mut self, other: &Self) -> bool {
            let mut changed = false;
            for i in 0..self.states.len() {
                let merged = self.states[i].union(other.states[i]);
                if merged != self.states[i] {
                    self.states[i] = merged;
                    changed = true;
                }
                let merged_h = self.history[i].union(other.history[i]);
                if merged_h != self.history[i] {
                    self.history[i] = merged_h;
                    changed = true;
                }
                let merged_dep = self.departed_from[i].union(other.departed_from[i]);
                if merged_dep != self.departed_from[i] {
                    self.departed_from[i] = merged_dep;
                    changed = true;
                }
                let rb = self.rolled_back[i] || other.rolled_back[i];
                if rb != self.rolled_back[i] {
                    self.rolled_back[i] = rb;
                    changed = true;
                }
                let ret = self.retired[i] && other.retired[i];
                if ret != self.retired[i] {
                    self.retired[i] = ret;
                    changed = true;
                }
            }
            for item in &other.invalidated {
                if self.invalidated.insert(item.clone()) {
                    changed = true;
                }
            }
            // Union may-alias and mutable-capability maps so merged references/closures stay conservative.
            for (k, other_set) in &other.param_alias {
                let entry = self.param_alias.entry(*k).or_default();
                for item in other_set {
                    if entry.insert(item.clone()) {
                        changed = true;
                    }
                }
            }
            for (k, other_set) in &other.may_alias {
                let entry = self.may_alias.entry(*k).or_default();
                for &h in other_set {
                    if entry.insert(h) {
                        changed = true;
                    }
                }
            }
            for (k, other_set) in &other.mut_cap_params {
                let entry = self.mut_cap_params.entry(*k).or_default();
                for item in other_set {
                    if entry.insert(item.clone()) {
                        changed = true;
                    }
                }
            }
            for (k, other_set) in &other.mut_cap_handles {
                let entry = self.mut_cap_handles.entry(*k).or_default();
                for &h in other_set {
                    if entry.insert(h) {
                        changed = true;
                    }
                }
            }
            // Intersect all must-alias / exact-fact maps so ambiguous branches fail closed.
            let prev_alias = self.alias.len();
            self.alias.retain(|k, v| other.alias.get(k) == Some(v));
            if self.alias.len() != prev_alias {
                changed = true;
            }
            let prev_sf = self.state_field_of.len();
            self.state_field_of
                .retain(|k, v| other.state_field_of.get(k) == Some(v));
            if self.state_field_of.len() != prev_sf {
                changed = true;
            }
            let prev_ev = self.enum_variant.len();
            self.enum_variant
                .retain(|k, v| other.enum_variant.get(k) == Some(v));
            if self.enum_variant.len() != prev_ev {
                changed = true;
            }
            let prev_ea = self.error_arm_const.len();
            self.error_arm_const
                .retain(|k, v| other.error_arm_const.get(k) == Some(v));
            if self.error_arm_const.len() != prev_ea {
                changed = true;
            }
            let prev_poll = self.poll_of.len();
            self.poll_of.retain(|k, v| other.poll_of.get(k) == Some(v));
            if self.poll_of.len() != prev_poll {
                changed = true;
            }
            let prev_eu = self.eu_of.len();
            self.eu_of.retain(|k, v| other.eu_of.get(k) == Some(v));
            if self.eu_of.len() != prev_eu {
                changed = true;
            }
            let prev_discr = self.discr_of.len();
            self.discr_of
                .retain(|k, v| other.discr_of.get(k) == Some(v));
            if self.discr_of.len() != prev_discr {
                changed = true;
            }
            changed
        }

        fn set_handle_state(&mut self, h_idx: usize, new_state: ProtocolStateSet) {
            let old_state = self.states[h_idx];
            let departed = old_state.subtract(new_state);
            self.departed_from[h_idx] = self.departed_from[h_idx].union(departed);
            if !new_state.intersect(self.departed_from[h_idx]).is_empty() {
                self.rolled_back[h_idx] = true;
            }
            self.states[h_idx] = new_state;
            self.history[h_idx] = self.history[h_idx].union(new_state);
        }

        /// Conservative weak update for a handle that *may* alias the written handle.
        /// Accounts for both the non-aliasing branch (state unchanged) and the aliasing branch
        /// (transition to `new_state`, recording departures and potential rollback).
        fn weak_update_handle_state(&mut self, h_idx: usize, new_state: ProtocolStateSet) {
            let aliased_departed = self.states[h_idx].subtract(new_state);
            self.departed_from[h_idx] = self.departed_from[h_idx].union(aliased_departed);
            if !new_state.intersect(self.departed_from[h_idx]).is_empty() {
                self.rolled_back[h_idx] = true;
            }
            self.states[h_idx] = self.states[h_idx].union(new_state);
            self.history[h_idx] = self.history[h_idx].union(new_state);
        }

        fn resolve_param_places(&self, place: &Place) -> ParamAliasSet {
            let mut out = ParamAliasSet::new();
            let PlaceRoot::Local(rl) = place.root else {
                return out;
            };
            let Some(prefixes) = self.param_alias.get(&rl) else {
                return out;
            };
            for (p_idx, prefix) in prefixes {
                let mut path = prefix.clone();
                for proj in &place.projections {
                    match proj {
                        Projection::Deref => {}
                        Projection::Field(f) => path.push(f.field),
                        Projection::Index(_)
                        | Projection::TupleField(_)
                        | Projection::ErrorArm(_)
                        | Projection::ConstIndex(_) => break,
                    }
                }
                out.insert((*p_idx, path));
            }
            out
        }

        fn record_place_invalidation(&mut self, place: &Place) {
            let PlaceRoot::Local(rl) = place.root else {
                return;
            };
            for (p_idx, path) in self.resolve_param_places(place) {
                if !place.projections.is_empty() || rl.index() != p_idx {
                    self.invalidated.insert((p_idx, path));
                }
            }
        }

        fn resolve_handle(&self, op: &Operand) -> Option<usize> {
            match op {
                Operand::Copy(p) | Operand::Move(p) if p.projections.is_empty() => match p.root {
                    PlaceRoot::Local(l) => self.alias.get(&l).copied(),
                    PlaceRoot::Capture(_) => None,
                },
                Operand::Copy(_) | Operand::Move(_) | Operand::Const(_) | Operand::Pooled(_) => {
                    None
                }
            }
        }

        fn invalidate_handle(&mut self, h_idx: usize) {
            self.poll_of.retain(|_, (h, _, _)| *h != h_idx);
            self.eu_of.retain(|_, entries| {
                entries.retain(|(h, _, _)| *h != h_idx);
                !entries.is_empty()
            });
            self.state_field_of.retain(|_, h| *h != h_idx);
            self.discr_of.retain(|_, t| {
                !matches!(
                    t,
                    DiscrTarget::HandleState(h) | DiscrTarget::ValidCheck { handle: h, .. }
                        if *h == h_idx
                )
            });
        }
    }

    let place_targets_state_field = |place: &Place, st: &ProtoState| -> Option<usize> {
        let PlaceRoot::Local(rl) = place.root else {
            return None;
        };
        let &h_idx = st.alias.get(&rl)?;
        let (_, sf) = local_info.get(h_idx).and_then(|o| *o)?.state_field?;
        let (last, prefix) = place.projections.split_last()?;
        if matches!(last, Projection::Field(f) if f.field == sf)
            && prefix.iter().all(|p| matches!(p, Projection::Deref))
        {
            Some(h_idx)
        } else {
            None
        }
    };

    let strip_indirection = |mut ty: zil_ir::body::TyId| -> zil_ir::body::TyId {
        for _ in 0..4 {
            match tys.get(ty) {
                Some(
                    TyKind::Ref { pointee, .. } | TyKind::Managed(pointee) | TyKind::Desc(pointee),
                ) => ty = *pointee,
                _ => break,
            }
        }
        ty
    };

    let mut entry_alias = HashMap::new();
    let mut entry_may_alias: HashMap<Local, BTreeSet<usize>> = HashMap::new();
    for (i, info) in local_info.iter().enumerate() {
        entry_alias.insert(Local(i as u32), i);
        if info.is_some() {
            let mut s = BTreeSet::new();
            s.insert(i);
            entry_may_alias.insert(Local(i as u32), s);
        }
    }
    let mut entry_param_alias: HashMap<Local, ParamAliasSet> = HashMap::new();
    for i in 0..param_count {
        let mut s = ParamAliasSet::new();
        s.insert((i, Vec::new()));
        entry_param_alias.insert(Local(i as u32), s);
    }
    // Cross-parameter Desc may-aliasing: two `Desc` parameters (or a `Desc` and a shared `&`
    // parameter) with compatible pointee types may alias at function entry unless the function
    // explicitly declares `requires(apart(a, b))`.
    for i in 0..param_count {
        for j in (i + 1)..param_count {
            let declared_apart = func.sig.state_contracts.iter().any(|c| {
                matches!(
                    *c,
                    StateContractClause::RequiresApart { param_a, param_b }
                        if (param_a as usize == i && param_b as usize == j)
                            || (param_a as usize == j && param_b as usize == i)
                )
            });
            if declared_apart {
                continue;
            }
            let ti = func.body.locals[i].ty;
            let tj = func.body.locals[j].ty;
            let is_desc = |ty| matches!(tys.get(ty), Some(TyKind::Desc(_)));
            let is_shared_ref = |ty| {
                matches!(
                    tys.get(ty),
                    Some(TyKind::Ref {
                        mutability: Mutability::Shared,
                        ..
                    })
                )
            };
            let pair_can_alias = (is_desc(ti) && (is_desc(tj) || is_shared_ref(tj)))
                || (is_desc(tj) && is_shared_ref(ti));
            if !pair_can_alias {
                continue;
            }
            let inner_i = strip_indirection(ti);
            let inner_j = strip_indirection(tj);
            let types_compatible = inner_i == inner_j
                || matches!(
                    tys.get(inner_i),
                    Some(TyKind::Param(_) | TyKind::Dyn(_) | TyKind::OwnedDyn { .. })
                )
                || matches!(
                    tys.get(inner_j),
                    Some(TyKind::Param(_) | TyKind::Dyn(_) | TyKind::OwnedDyn { .. })
                );
            if types_compatible {
                entry_param_alias
                    .entry(Local(i as u32))
                    .or_default()
                    .insert((j, Vec::new()));
                entry_param_alias
                    .entry(Local(j as u32))
                    .or_default()
                    .insert((i, Vec::new()));
                if local_info[i].is_some() && local_info[j].is_some() {
                    entry_may_alias
                        .entry(Local(i as u32))
                        .or_default()
                        .insert(j);
                    entry_may_alias
                        .entry(Local(j as u32))
                        .or_default()
                        .insert(i);
                }
            }
        }
    }
    let entry_init_states = init_states.clone();
    let entry_state = ProtoState {
        states: init_states.clone(),
        history: init_states,
        departed_from: vec![ProtocolStateSet(0); nlocals],
        rolled_back: vec![false; nlocals],
        retired: vec![false; nlocals],
        invalidated: ParamAliasSet::new(),
        param_alias: entry_param_alias,
        alias: entry_alias,
        may_alias: entry_may_alias,
        mut_cap_params: HashMap::new(),
        mut_cap_handles: HashMap::new(),
        state_field_of: HashMap::new(),
        enum_variant: HashMap::new(),
        error_arm_const: HashMap::new(),
        poll_of: HashMap::new(),
        eu_of: HashMap::new(),
        discr_of: HashMap::new(),
    };

    let nblocks = func.body.blocks.len();
    let mut in_states: Vec<Option<ProtoState>> = vec![None; nblocks];
    let mut queue: VecDeque<usize> = VecDeque::new();
    if nblocks > 0 {
        in_states[0] = Some(entry_state);
        queue.push_back(0);
    }

    let enqueue = |target: BlockId,
                   st: ProtoState,
                   in_states: &mut [Option<ProtoState>],
                   queue: &mut VecDeque<usize>| {
        let ti = target.index();
        if ti >= in_states.len() {
            return;
        }
        match &mut in_states[ti] {
            None => {
                in_states[ti] = Some(st);
                queue.push_back(ti);
            }
            Some(existing) => {
                if existing.join(&st) {
                    queue.push_back(ti);
                }
            }
        }
    };

    let mut steps: u64 = 0;
    while let Some(bi) = queue.pop_front() {
        steps += 1;
        if steps > budget.steps.max(1024) {
            return Err(CheckError {
                function: fn_id,
                kind: CheckErrorKind::ResourceLimit,
            });
        }
        let mut st = in_states[bi].clone().expect("enqueued block has state");
        let block = &func.body.blocks[bi];

        for inst in &block.instructions {
            match &inst.kind {
                InstructionKind::Drop { place } => {
                    st.record_place_invalidation(place);
                    continue;
                }
                InstructionKind::Replace { place, value, old } => {
                    // `Replace` mutates `place` (e.g. assigning to an owning/Drop field)
                    // and moves the previous instance into `old`.
                    st.record_place_invalidation(place);
                    // Clear stale alias/value facts on `old`.
                    st.alias.remove(old);
                    st.may_alias.remove(old);
                    st.param_alias.remove(old);
                    st.mut_cap_params.remove(old);
                    st.mut_cap_handles.remove(old);
                    st.state_field_of.remove(old);
                    st.enum_variant.remove(old);
                    st.error_arm_const.remove(old);
                    st.poll_of.remove(old);
                    st.eu_of.remove(old);
                    st.discr_of.remove(old);
                    if let Some(h_idx) = place_targets_state_field(place, &st) {
                        let vc = local_info[h_idx].map_or(3, |i| i.variant_count);
                        let new_state = match value {
                            Operand::Copy(src) | Operand::Move(src)
                                if src.projections.is_empty() =>
                            {
                                if let PlaceRoot::Local(sl) = src.root
                                    && let Some(&v) = st.enum_variant.get(&sl)
                                {
                                    ProtocolStateSet::single(v)
                                } else {
                                    ProtocolStateSet::all_variants(vc)
                                }
                            }
                            Operand::Copy(_)
                            | Operand::Move(_)
                            | Operand::Const(_)
                            | Operand::Pooled(_) => ProtocolStateSet::all_variants(vc),
                        };
                        st.set_handle_state(h_idx, new_state);
                        st.invalidate_handle(h_idx);
                        if let PlaceRoot::Local(rl) = place.root
                            && let Some(targets) = st.may_alias.get(&rl).cloned()
                        {
                            for other_h in targets {
                                if other_h != h_idx {
                                    st.weak_update_handle_state(other_h, new_state);
                                    st.invalidate_handle(other_h);
                                }
                            }
                        }
                    } else if let PlaceRoot::Local(rl) = place.root {
                        let may_targets = st.may_alias.get(&rl).cloned();
                        if let Some(targets) = may_targets {
                            let is_deref_only = !place.projections.is_empty()
                                && place
                                    .projections
                                    .iter()
                                    .all(|p| matches!(p, Projection::Deref));
                            let last_field = place.projections.last().and_then(|p| match p {
                                Projection::Field(f) => Some(f.field),
                                Projection::Deref
                                | Projection::Index(_)
                                | Projection::TupleField(_)
                                | Projection::ErrorArm(_)
                                | Projection::ConstIndex(_) => None,
                            });
                            for h_idx in targets {
                                if let Some(Some(info)) = local_info.get(h_idx) {
                                    let touches_state = is_deref_only
                                        || match (info.state_field, last_field) {
                                            (None, _) => true,
                                            (Some((_, sf)), Some(lf)) => sf == lf,
                                            (Some(_), None) => true,
                                        };
                                    if touches_state {
                                        let all =
                                            ProtocolStateSet::all_variants(info.variant_count);
                                        st.set_handle_state(h_idx, all);
                                        st.invalidate_handle(h_idx);
                                    }
                                }
                            }
                        }
                    }
                    continue;
                }
                InstructionKind::StorageDead(dl) => {
                    st.alias.remove(dl);
                    st.may_alias.remove(dl);
                    st.param_alias.remove(dl);
                    st.mut_cap_params.remove(dl);
                    st.mut_cap_handles.remove(dl);
                    st.state_field_of.remove(dl);
                    st.enum_variant.remove(dl);
                    st.error_arm_const.remove(dl);
                    st.poll_of.remove(dl);
                    st.eu_of.remove(dl);
                    st.discr_of.remove(dl);
                    continue;
                }
                InstructionKind::StorageLive(_)
                | InstructionKind::ScopeEnter(_)
                | InstructionKind::ScopeExit { .. }
                | InstructionKind::RegisterDefer { .. } => {
                    continue;
                }
                InstructionKind::Assign { dest, value } => {
                    // Always record parameter-projection invalidations on any write.
                    st.record_place_invalidation(dest);

                    // Case A: Writing directly to a protocol handle's `state` field (`h.state = ...`).
                    if let Some(h_idx) = place_targets_state_field(dest, &st) {
                        let vc = local_info[h_idx].map_or(3, |i| i.variant_count);
                        let new_state = if let Rvalue::Aggregate {
                            kind: AggregateKind::Adt { variant, .. },
                            ..
                        } = value
                        {
                            ProtocolStateSet::single(variant.0)
                        } else if let Rvalue::Use(Operand::Copy(src) | Operand::Move(src)) = value
                            && src.projections.is_empty()
                            && let PlaceRoot::Local(sl) = src.root
                            && let Some(&v) = st.enum_variant.get(&sl)
                        {
                            ProtocolStateSet::single(v)
                        } else {
                            ProtocolStateSet::all_variants(vc)
                        };
                        st.set_handle_state(h_idx, new_state);
                        st.invalidate_handle(h_idx);
                        if let PlaceRoot::Local(rl) = dest.root
                            && let Some(targets) = st.may_alias.get(&rl).cloned()
                        {
                            for other_h in targets {
                                if other_h != h_idx {
                                    st.weak_update_handle_state(other_h, new_state);
                                    st.invalidate_handle(other_h);
                                }
                            }
                        }
                        continue;
                    }

                    // Case A1b: Writing through an AMBIGUOUS merged reference (`r.state = ...` or `*r = ...`).
                    if !dest.projections.is_empty()
                        && let PlaceRoot::Local(rl) = dest.root
                        && !st.alias.contains_key(&rl)
                        && let Some(targets) = st.may_alias.get(&rl).cloned()
                    {
                        for h_idx in targets {
                            if let Some(Some(info)) = local_info.get(h_idx) {
                                let touches_state = match info.state_field {
                                    None => true,
                                    Some((_, sf)) => dest
                                        .projections
                                        .iter()
                                        .all(|p| matches!(p, Projection::Deref))
                                        || dest.projections.iter().any(
                                            |p| matches!(p, Projection::Field(f) if f.field == sf),
                                        ),
                                };
                                if touches_state {
                                    let all = ProtocolStateSet::all_variants(info.variant_count);
                                    st.set_handle_state(h_idx, all);
                                    st.invalidate_handle(h_idx);
                                }
                            }
                        }
                        continue;
                    }

                    // Case A2: Writing through a reference to a protocol handle (`*h = value`).
                    if !dest.projections.is_empty()
                        && dest
                            .projections
                            .iter()
                            .all(|p| matches!(p, Projection::Deref))
                        && let PlaceRoot::Local(rl) = dest.root
                        && let Some(&h_idx) = st.alias.get(&rl)
                        && let Some(Some(info)) = local_info.get(h_idx)
                    {
                        let new_state = if let Rvalue::Aggregate {
                            kind: AggregateKind::Adt { variant, .. },
                            operands,
                        } = value
                        {
                            if let Some((_, sf)) = info.state_field {
                                operands
                                    .get(sf.0 as usize)
                                    .and_then(|op| match op {
                                        Operand::Copy(p) | Operand::Move(p)
                                            if p.projections.is_empty() =>
                                        {
                                            if let PlaceRoot::Local(sl) = p.root {
                                                st.enum_variant
                                                    .get(&sl)
                                                    .map(|&v| ProtocolStateSet::single(v))
                                            } else {
                                                None
                                            }
                                        }
                                        Operand::Copy(_)
                                        | Operand::Move(_)
                                        | Operand::Const(_)
                                        | Operand::Pooled(_) => None,
                                    })
                                    .unwrap_or_else(|| {
                                        ProtocolStateSet::all_variants(info.variant_count)
                                    })
                            } else {
                                ProtocolStateSet::single(variant.0)
                            }
                        } else if let Rvalue::Use(op) = value {
                            st.resolve_handle(op).map_or_else(
                                || ProtocolStateSet::all_variants(info.variant_count),
                                |src_h| st.states[src_h],
                            )
                        } else {
                            ProtocolStateSet::all_variants(info.variant_count)
                        };
                        st.set_handle_state(h_idx, new_state);
                        st.invalidate_handle(h_idx);
                        if let Some(targets) = st.may_alias.get(&rl).cloned() {
                            for other_h in targets {
                                if other_h != h_idx {
                                    st.weak_update_handle_state(other_h, new_state);
                                    st.invalidate_handle(other_h);
                                }
                            }
                        }
                        continue;
                    }

                    // Case B: Whole-local assignment `dl = value`.
                    if dest.projections.is_empty()
                        && let PlaceRoot::Local(dl) = dest.root
                    {
                        let di = dl.index();
                        st.alias.remove(&dl);
                        st.may_alias.remove(&dl);
                        st.param_alias.remove(&dl);
                        st.mut_cap_params.remove(&dl);
                        st.mut_cap_handles.remove(&dl);
                        if di < param_count {
                            let mut s = ParamAliasSet::new();
                            s.insert((di, Vec::new()));
                            st.param_alias.insert(dl, s);
                        }
                        if !matches!(
                            tys.get(func.body.locals[di].ty),
                            Some(TyKind::Ref { .. } | TyKind::Desc(_))
                        ) {
                            st.alias.insert(dl, di);
                        }
                        if local_info.get(di).is_some_and(Option::is_some) {
                            st.alias.insert(dl, di);
                            let mut s = BTreeSet::new();
                            s.insert(di);
                            st.may_alias.insert(dl, s);
                            st.invalidate_handle(di);
                        }
                        st.state_field_of.remove(&dl);
                        st.enum_variant.remove(&dl);
                        st.error_arm_const.remove(&dl);
                        st.poll_of.remove(&dl);
                        st.eu_of.remove(&dl);
                        st.discr_of.remove(&dl);
                        match value {
                            Rvalue::Use(Operand::Copy(src) | Operand::Move(src)) => {
                                if let PlaceRoot::Local(sl) = src.root {
                                    if let Some(mcp) = st.mut_cap_params.get(&sl).cloned() {
                                        st.mut_cap_params.insert(dl, mcp);
                                    }
                                    if let Some(mch) = st.mut_cap_handles.get(&sl).cloned() {
                                        st.mut_cap_handles.insert(dl, mch);
                                    }
                                }
                                if src.projections.is_empty()
                                    && let PlaceRoot::Local(sl) = src.root
                                    && let Some(pa) = st.param_alias.get(&sl).cloned()
                                    && matches!(
                                        tys.get(func.body.locals[di].ty),
                                        Some(TyKind::Ref { .. } | TyKind::Desc(_))
                                    )
                                {
                                    st.param_alias.insert(dl, pa);
                                }
                                if let Some(h_idx) = place_targets_state_field(src, &st) {
                                    st.state_field_of.insert(dl, h_idx);
                                } else if src.projections.is_empty()
                                    && let PlaceRoot::Local(sl) = src.root
                                {
                                    if let Some(ma) = st.may_alias.get(&sl).cloned() {
                                        let is_copy_value =
                                            matches!(value, Rvalue::Use(Operand::Copy(_)))
                                                && !matches!(
                                                    tys.get(func.body.locals[di].ty),
                                                    Some(TyKind::Ref { .. } | TyKind::Desc(_))
                                                );
                                        if !is_copy_value {
                                            st.may_alias.insert(dl, ma);
                                        }
                                    }
                                    if let Some(&h) = st.alias.get(&sl) {
                                        let is_copy_value =
                                            matches!(value, Rvalue::Use(Operand::Copy(_)))
                                                && !matches!(
                                                    tys.get(func.body.locals[di].ty),
                                                    Some(TyKind::Ref { .. } | TyKind::Desc(_))
                                                );
                                        if is_copy_value
                                            && local_info.get(di).is_some_and(Option::is_some)
                                        {
                                            // By-value Copy of a value-type protocol handle creates an independent copy.
                                            st.states[di] = st.states[h];
                                            st.retired[di] = st.retired[h];
                                            st.alias.insert(dl, di);
                                            let mut s = BTreeSet::new();
                                            s.insert(di);
                                            st.may_alias.insert(dl, s);
                                        } else {
                                            // Move of an owning handle or copy of &/&mut/Desc preserves handle identity.
                                            st.alias.insert(dl, h);
                                        }
                                    } else if matches!(
                                        tys.get(func.body.locals[di].ty),
                                        Some(TyKind::Ref { .. } | TyKind::Desc(_))
                                    ) {
                                        // Source reference had ambiguous alias; destination must also be ambiguous!
                                        st.alias.remove(&dl);
                                    }
                                    if let Some(&sf) = st.state_field_of.get(&sl) {
                                        st.state_field_of.insert(dl, sf);
                                    }
                                    if let Some(&ev) = st.enum_variant.get(&sl) {
                                        st.enum_variant.insert(dl, ev);
                                    }
                                    if let Some(&ea) = st.error_arm_const.get(&sl) {
                                        st.error_arm_const.insert(dl, ea);
                                    }
                                    if let Some(&po) = st.poll_of.get(&sl) {
                                        st.poll_of.insert(dl, po);
                                    }
                                    if let Some(eu) = st.eu_of.get(&sl).cloned() {
                                        st.eu_of.insert(dl, eu);
                                    }
                                }
                            }
                            Rvalue::Ref { place: src, .. } | Rvalue::Describe { place: src } => {
                                if let PlaceRoot::Local(sl) = src.root {
                                    if let Some(mcp) = st.mut_cap_params.get(&sl).cloned() {
                                        st.mut_cap_params.insert(dl, mcp);
                                    }
                                    if let Some(mch) = st.mut_cap_handles.get(&sl).cloned() {
                                        st.mut_cap_handles.insert(dl, mch);
                                    }
                                }
                                let pa = st.resolve_param_places(src);
                                if !pa.is_empty() {
                                    st.param_alias.insert(dl, pa);
                                }
                                if src
                                    .projections
                                    .iter()
                                    .all(|p| matches!(p, Projection::Deref))
                                    && let PlaceRoot::Local(sl) = src.root
                                {
                                    if let Some(&h) = st.alias.get(&sl) {
                                        st.alias.insert(dl, h);
                                    } else {
                                        st.alias.remove(&dl);
                                    }
                                    if let Some(ma) = st.may_alias.get(&sl).cloned() {
                                        st.may_alias.insert(dl, ma);
                                    }
                                }
                            }
                            Rvalue::Discriminant(src) => {
                                if let Some(h_idx) = place_targets_state_field(src, &st) {
                                    st.discr_of.insert(dl, DiscrTarget::HandleState(h_idx));
                                } else if src.projections.is_empty()
                                    && let PlaceRoot::Local(sl) = src.root
                                {
                                    if let Some(&h_idx) = st.state_field_of.get(&sl) {
                                        st.discr_of.insert(dl, DiscrTarget::HandleState(h_idx));
                                    } else if st.poll_of.contains_key(&sl) {
                                        st.discr_of.insert(dl, DiscrTarget::Poll(sl));
                                    } else if let Some(&h_idx) = st.alias.get(&sl)
                                        && local_info
                                            .get(h_idx)
                                            .and_then(|o| *o)
                                            .is_some_and(|i| i.state_field.is_none())
                                    {
                                        st.discr_of.insert(dl, DiscrTarget::HandleState(h_idx));
                                    } else {
                                        st.discr_of.insert(dl, DiscrTarget::Poll(sl));
                                    }
                                }
                            }
                            Rvalue::ErrorArm(src) => {
                                if src.projections.is_empty()
                                    && let PlaceRoot::Local(sl) = src.root
                                {
                                    st.discr_of.insert(dl, DiscrTarget::ErrorUnion(sl));
                                }
                            }
                            Rvalue::ErrorUnion {
                                ok, value: operand, ..
                            } => {
                                st.error_arm_const.insert(dl, *ok);
                                if let Operand::Copy(p) | Operand::Move(p) = operand
                                    && let PlaceRoot::Local(sl) = p.root
                                {
                                    if let Some(mcp) = st.mut_cap_params.get(&sl).cloned() {
                                        st.mut_cap_params.insert(dl, mcp);
                                    }
                                    if let Some(mch) = st.mut_cap_handles.get(&sl).cloned() {
                                        st.mut_cap_handles.insert(dl, mch);
                                    }
                                }
                            }
                            Rvalue::Aggregate { kind, operands } => {
                                // Track mutable capabilities captured inside closures or wrapped in aggregates.
                                let mut cap_params = ParamAliasSet::new();
                                let mut cap_handles = BTreeSet::new();
                                for op in operands {
                                    if let Operand::Copy(p) | Operand::Move(p) = op
                                        && let PlaceRoot::Local(sl) = p.root
                                    {
                                        if let Some(mcp) = st.mut_cap_params.get(&sl) {
                                            cap_params.extend(mcp.iter().cloned());
                                        }
                                        if let Some(mch) = st.mut_cap_handles.get(&sl) {
                                            cap_handles.extend(mch.iter().copied());
                                        }
                                        if p.projections.is_empty()
                                            && func
                                                .body
                                                .locals
                                                .get(sl.index())
                                                .is_some_and(|ld| is_mut_ref_or_desc_ty(ld.ty))
                                        {
                                            if let Some(pa) = st.param_alias.get(&sl) {
                                                cap_params.extend(pa.iter().cloned());
                                            }
                                            if let Some(ma) = st.may_alias.get(&sl) {
                                                cap_handles.extend(ma.iter().copied());
                                            }
                                        }
                                    }
                                }
                                if !cap_params.is_empty() {
                                    st.mut_cap_params.insert(dl, cap_params);
                                }
                                if !cap_handles.is_empty() {
                                    st.mut_cap_handles.insert(dl, cap_handles);
                                }
                                if let AggregateKind::Adt { variant, .. } = kind {
                                    st.enum_variant.insert(dl, variant.0);
                                    if let Some(Some(info)) = local_info.get(di) {
                                        if let Some((_, sf)) = info.state_field {
                                            let field_state = operands
                                                .get(sf.0 as usize)
                                                .and_then(|op| match op {
                                                    Operand::Copy(p) | Operand::Move(p)
                                                        if p.projections.is_empty() =>
                                                    {
                                                        if let PlaceRoot::Local(sl) = p.root {
                                                            st.enum_variant.get(&sl).map(|&v| {
                                                                ProtocolStateSet::single(v)
                                                            })
                                                        } else {
                                                            None
                                                        }
                                                    }
                                                    Operand::Copy(_)
                                                    | Operand::Move(_)
                                                    | Operand::Const(_)
                                                    | Operand::Pooled(_) => None,
                                                })
                                                .unwrap_or_else(|| {
                                                    ProtocolStateSet::all_variants(
                                                        info.variant_count,
                                                    )
                                                });
                                            st.states[di] = field_state;
                                        } else {
                                            st.states[di] = ProtocolStateSet::single(variant.0);
                                        }
                                    }
                                }
                            }
                            Rvalue::Convert { operand, to } => {
                                if let Operand::Copy(p) | Operand::Move(p) = operand
                                    && let PlaceRoot::Local(sl) = p.root
                                {
                                    let mut cap_params =
                                        st.mut_cap_params.get(&sl).cloned().unwrap_or_default();
                                    let mut cap_handles =
                                        st.mut_cap_handles.get(&sl).cloned().unwrap_or_default();
                                    if p.projections.is_empty() {
                                        if matches!(
                                            tys.get(*to),
                                            Some(TyKind::Ref { .. } | TyKind::Desc(_))
                                        ) {
                                            if let Some(pa) = st.param_alias.get(&sl).cloned() {
                                                st.param_alias.insert(dl, pa);
                                            }
                                            if let Some(ma) = st.may_alias.get(&sl).cloned() {
                                                st.may_alias.insert(dl, ma);
                                            }
                                            if let Some(&h) = st.alias.get(&sl) {
                                                st.alias.insert(dl, h);
                                            } else {
                                                st.alias.remove(&dl);
                                            }
                                        } else if matches!(
                                            tys.get(*to),
                                            Some(TyKind::OwnedDyn { .. })
                                        ) && func
                                            .body
                                            .locals
                                            .get(sl.index())
                                            .is_some_and(|ld| is_mut_ref_or_desc_ty(ld.ty))
                                        {
                                            if let Some(pa) = st.param_alias.get(&sl) {
                                                cap_params.extend(pa.iter().cloned());
                                            }
                                            if let Some(ma) = st.may_alias.get(&sl) {
                                                cap_handles.extend(ma.iter().copied());
                                            }
                                        }
                                    }
                                    if !cap_params.is_empty() {
                                        st.mut_cap_params.insert(dl, cap_params);
                                    }
                                    if !cap_handles.is_empty() {
                                        st.mut_cap_handles.insert(dl, cap_handles);
                                    }
                                }
                            }
                            Rvalue::Unary {
                                op: zil_ir::body::UnOp::Not,
                                operand,
                            } => {
                                if let Operand::Copy(p) | Operand::Move(p) = operand
                                    && p.projections.is_empty()
                                    && let PlaceRoot::Local(sl) = p.root
                                    && let Some(DiscrTarget::ValidCheck { handle, inverted }) =
                                        st.discr_of.get(&sl).copied()
                                {
                                    st.discr_of.insert(
                                        dl,
                                        DiscrTarget::ValidCheck {
                                            handle,
                                            inverted: !inverted,
                                        },
                                    );
                                }
                            }
                            Rvalue::Use(_)
                            | Rvalue::SliceRef { .. }
                            | Rvalue::Unary { .. }
                            | Rvalue::Binary { .. }
                            | Rvalue::Len(_)
                            | Rvalue::FnRef(_)
                            | Rvalue::ForeignFnRef(_)
                            | Rvalue::StaticUtf(_) => {}
                        }
                    }
                }
            }
        }

        match &block.terminator {
            Terminator::Goto { target } => enqueue(*target, st, &mut in_states, &mut queue),
            Terminator::Assert { target, .. } => enqueue(*target, st, &mut in_states, &mut queue),
            Terminator::Switch {
                discr,
                arms,
                otherwise,
            } => {
                let target_info = match discr {
                    Operand::Copy(p) | Operand::Move(p) if p.projections.is_empty() => {
                        match p.root {
                            PlaceRoot::Local(l) => st.discr_of.get(&l).copied(),
                            PlaceRoot::Capture(_) => None,
                        }
                    }
                    Operand::Copy(_)
                    | Operand::Move(_)
                    | Operand::Const(_)
                    | Operand::Pooled(_) => None,
                };
                let mut covered_mask = ProtocolStateSet(0);
                let mut covered_ok = false;
                let mut covered_err = false;
                let mut covered_valid_true = false;
                for &(val, target) in arms {
                    let mut branch_st = st.clone();
                    match target_info {
                        Some(DiscrTarget::Poll(pl)) => {
                            if let Some((h_idx, pend_st, ready_st)) = st.poll_of.get(&pl).copied() {
                                if val == 0 {
                                    branch_st.states[h_idx] = pend_st;
                                } else if val == 1 {
                                    branch_st.states[h_idx] = ready_st;
                                }
                            }
                        }
                        Some(DiscrTarget::ErrorUnion(eu_l)) => {
                            if let Some(entries) = st.eu_of.get(&eu_l) {
                                for &(h_idx, ok_st, err_st) in entries {
                                    if val == 1 {
                                        covered_ok = true;
                                        if let Some(s) = ok_st {
                                            branch_st.states[h_idx] = s;
                                        }
                                    } else if val == 0 {
                                        covered_err = true;
                                        if let Some(s) = err_st {
                                            branch_st.states[h_idx] = s;
                                        }
                                    }
                                }
                            }
                        }
                        Some(DiscrTarget::HandleState(h_idx)) => {
                            let single = ProtocolStateSet::single(val as u32);
                            covered_mask = covered_mask.union(single);
                            let refined = branch_st.states[h_idx].intersect(single);
                            if refined.is_empty() {
                                continue;
                            }
                            branch_st.states[h_idx] = refined;
                        }
                        Some(DiscrTarget::ValidCheck { handle, inverted }) => {
                            let is_invalid = (val == 0) != inverted;
                            if is_invalid {
                                branch_st.states[handle] =
                                    ProtocolStateSet(ProtocolStateSet::CLOSED);
                            } else {
                                covered_valid_true = true;
                            }
                        }
                        None => {}
                    }
                    enqueue(target, branch_st, &mut in_states, &mut queue);
                }
                let mut other_st = st;
                match target_info {
                    Some(DiscrTarget::HandleState(h_idx)) => {
                        let rem = other_st.states[h_idx].subtract(covered_mask);
                        if !rem.is_empty() {
                            other_st.states[h_idx] = rem;
                            enqueue(*otherwise, other_st, &mut in_states, &mut queue);
                        }
                    }
                    Some(DiscrTarget::ErrorUnion(eu_l)) => {
                        if let Some(entries) = other_st.eu_of.get(&eu_l).cloned() {
                            for (h_idx, ok_st, err_st) in entries {
                                if covered_ok && !covered_err {
                                    if let Some(s) = err_st {
                                        other_st.states[h_idx] = s;
                                    }
                                } else if covered_err
                                    && !covered_ok
                                    && let Some(s) = ok_st
                                {
                                    other_st.states[h_idx] = s;
                                }
                            }
                        }
                        enqueue(*otherwise, other_st, &mut in_states, &mut queue);
                    }
                    Some(DiscrTarget::ValidCheck { handle, .. }) => {
                        if covered_valid_true {
                            other_st.states[handle] = ProtocolStateSet(ProtocolStateSet::CLOSED);
                        }
                        enqueue(*otherwise, other_st, &mut in_states, &mut queue);
                    }
                    Some(DiscrTarget::Poll(_)) | None => {
                        enqueue(*otherwise, other_st, &mut in_states, &mut queue);
                    }
                }
            }
            Terminator::Call {
                callee,
                args,
                dest,
                target,
                cancel,
            } => {
                if let Some(cancel_target) = cancel {
                    enqueue(*cancel_target, st.clone(), &mut in_states, &mut queue);
                }
                st.record_place_invalidation(dest);
                let dest_local = if dest.projections.is_empty() {
                    match dest.root {
                        PlaceRoot::Local(l) => Some(l),
                        PlaceRoot::Capture(_) => None,
                    }
                } else {
                    None
                };
                if let Some(dl) = dest_local {
                    st.mut_cap_params.remove(&dl);
                    st.mut_cap_handles.remove(&dl);
                }
                let apply_local_effects =
                    |st: &mut ProtoState,
                     local: Local,
                     include_direct_mut_or_desc: bool,
                     excluded_handles: &[usize]| {
                        if let Some(mcp) = st.mut_cap_params.get(&local).cloned() {
                            for item in mcp {
                                st.invalidated.insert(item);
                            }
                        }
                        if let Some(mch) = st.mut_cap_handles.get(&local).cloned() {
                            for h_idx in mch {
                                if !excluded_handles.contains(&h_idx)
                                    && let Some(Some(info)) = local_info.get(h_idx)
                                {
                                    st.set_handle_state(
                                        h_idx,
                                        ProtocolStateSet::all_variants(info.variant_count),
                                    );
                                    st.rolled_back[h_idx] = true;
                                    st.invalidate_handle(h_idx);
                                }
                            }
                        }
                        if include_direct_mut_or_desc
                            && func
                                .body
                                .locals
                                .get(local.index())
                                .is_some_and(|ld| is_mut_ref_or_desc_ty(ld.ty))
                        {
                            if let Some(pa_set) = st.param_alias.get(&local).cloned() {
                                for item in pa_set {
                                    st.invalidated.insert(item);
                                }
                            }
                            if let Some(ma_set) = st.may_alias.get(&local).cloned() {
                                for h_idx in ma_set {
                                    if !excluded_handles.contains(&h_idx)
                                        && let Some(Some(info)) = local_info.get(h_idx)
                                    {
                                        st.set_handle_state(
                                            h_idx,
                                            ProtocolStateSet::all_variants(info.variant_count),
                                        );
                                        st.rolled_back[h_idx] = true;
                                        st.invalidate_handle(h_idx);
                                    }
                                }
                            }
                        }
                    };
                let apply_place_effects =
                    |st: &mut ProtoState,
                     place: &Place,
                     include_direct_mut_or_desc: bool,
                     excluded_handles: &[usize]| {
                        if let PlaceRoot::Local(al) = place.root {
                            if place.projections.is_empty() {
                                apply_local_effects(
                                    st,
                                    al,
                                    include_direct_mut_or_desc,
                                    excluded_handles,
                                );
                            } else {
                                apply_local_effects(st, al, true, excluded_handles);
                                if include_direct_mut_or_desc {
                                    st.record_place_invalidation(place);
                                }
                            }
                        }
                    };
                let apply_operand_effects =
                    |st: &mut ProtoState,
                     op: &Operand,
                     include_direct_mut_or_desc: bool,
                     excluded_handles: &[usize]| {
                        if let Operand::Copy(p) | Operand::Move(p) = op {
                            apply_place_effects(
                                st,
                                p,
                                include_direct_mut_or_desc,
                                excluded_handles,
                            );
                        }
                    };
                let apply_all_args = |st: &mut ProtoState, excluded_handles: &[usize]| {
                    for arg in args {
                        apply_operand_effects(st, arg, true, excluded_handles);
                    }
                };
                match callee {
                    Callee::Intrinsic(IntrinsicOp::OwnedDynCreate) => {
                        if let Some(dl) = dest_local {
                            let mut cap_params = ParamAliasSet::new();
                            let mut cap_handles = BTreeSet::new();
                            for arg in args {
                                if let Operand::Copy(p) | Operand::Move(p) = arg
                                    && let PlaceRoot::Local(al) = p.root
                                {
                                    if let Some(mcp) = st.mut_cap_params.get(&al) {
                                        cap_params.extend(mcp.iter().cloned());
                                    }
                                    if let Some(mch) = st.mut_cap_handles.get(&al) {
                                        cap_handles.extend(mch.iter().copied());
                                    }
                                    if p.projections.is_empty()
                                        && func
                                            .body
                                            .locals
                                            .get(al.index())
                                            .is_some_and(|ld| is_mut_ref_or_desc_ty(ld.ty))
                                    {
                                        if let Some(pa) = st.param_alias.get(&al) {
                                            cap_params.extend(pa.iter().cloned());
                                        }
                                        if let Some(ma) = st.may_alias.get(&al) {
                                            cap_handles.extend(ma.iter().copied());
                                        }
                                    }
                                }
                            }
                            if !cap_params.is_empty() {
                                st.mut_cap_params.insert(dl, cap_params);
                            }
                            if !cap_handles.is_empty() {
                                st.mut_cap_handles.insert(dl, cap_handles);
                            }
                        }
                    }
                    Callee::Intrinsic(
                        IntrinsicOp::StoreLen
                        | IntrinsicOp::StoreCapacity
                        | IntrinsicOp::StoreBorrow {
                            mutability: Mutability::Shared,
                        },
                    ) => {}
                    Callee::Intrinsic(IntrinsicOp::AsyncRegistrationValid) => {
                        if let Some(a0) = args.first()
                            && let Some(h_idx) = st.resolve_handle(a0)
                            && let Some(dl) = dest_local
                        {
                            st.discr_of.insert(
                                dl,
                                DiscrTarget::ValidCheck {
                                    handle: h_idx,
                                    inverted: false,
                                },
                            );
                        }
                    }
                    Callee::Intrinsic(
                        IntrinsicOp::TaskRequestCancel | IntrinsicOp::AsyncRegistrationRequestClose,
                    ) => {
                        if let Some(a0) = args.first()
                            && let Some(h_idx) = st.resolve_handle(a0)
                        {
                            let cur = st.states[h_idx];
                            let mut next = ProtocolStateSet(0);
                            if (cur.0 & (ProtocolStateSet::OPEN | ProtocolStateSet::CLOSING)) != 0 {
                                next = next.union(ProtocolStateSet(
                                    ProtocolStateSet::CLOSING | ProtocolStateSet::CLOSED,
                                ));
                            }
                            if (cur.0 & ProtocolStateSet::CLOSED) != 0 {
                                next = next.union(ProtocolStateSet(ProtocolStateSet::CLOSED));
                            }
                            st.states[h_idx] = next;
                            st.invalidate_handle(h_idx);
                        }
                    }
                    Callee::Intrinsic(
                        IntrinsicOp::TaskPollClose | IntrinsicOp::AsyncRegistrationPollClose,
                    ) => {
                        if let Some(a0) = args.first()
                            && let Some(h_idx) = st.resolve_handle(a0)
                        {
                            st.states[h_idx] = ProtocolStateSet(
                                ProtocolStateSet::CLOSING | ProtocolStateSet::CLOSED,
                            );
                            st.invalidate_handle(h_idx);
                            if let Some(dl) = dest_local {
                                st.poll_of.insert(
                                    dl,
                                    (
                                        h_idx,
                                        ProtocolStateSet(ProtocolStateSet::CLOSING),
                                        ProtocolStateSet(ProtocolStateSet::CLOSED),
                                    ),
                                );
                            }
                        }
                    }
                    Callee::Intrinsic(
                        IntrinsicOp::TaskFinishDestroy
                        | IntrinsicOp::AsyncRegistrationFinishDestroy,
                    ) => {
                        if let Some(a0) = args.first()
                            && let Some(h_idx) = st.resolve_handle(a0)
                        {
                            if !st.states[h_idx]
                                .is_subset_of(ProtocolStateSet(ProtocolStateSet::CLOSED))
                            {
                                return Err(fail(
                                    "finish_destroy called on protocol handle not proved Closed",
                                ));
                            }
                            st.retired[h_idx] = true;
                            st.invalidate_handle(h_idx);
                        }
                    }
                    Callee::Intrinsic(IntrinsicOp::RegistrationClose) => {
                        if let Some(a0) = args.first()
                            && let Some(h_idx) = st.resolve_handle(a0)
                        {
                            st.retired[h_idx] = true;
                            st.invalidate_handle(h_idx);
                        }
                    }
                    Callee::Direct(cid) => {
                        if let Some(callee_fn) = module.functions.get(cid.index())
                            && !callee_fn.sig.state_contracts.is_empty()
                        {
                            let mut pend_by_handle: HashMap<usize, ProtocolStateSet> =
                                HashMap::new();
                            let mut ready_by_handle: HashMap<usize, ProtocolStateSet> =
                                HashMap::new();
                            let mut ok_by_handle: HashMap<usize, ProtocolStateSet> = HashMap::new();
                            let mut err_by_handle: HashMap<usize, ProtocolStateSet> =
                                HashMap::new();
                            // 1. Check all preconditions BEFORE applying postconditions.
                            for c in &callee_fn.sig.state_contracts {
                                match *c {
                                    StateContractClause::RequiresState { param, states } => {
                                        let Some(arg) = args.get(param as usize) else {
                                            return Err(fail(
                                                "callee requires(state(...)) argument missing",
                                            ));
                                        };
                                        let Some(h_idx) = st.resolve_handle(arg) else {
                                            return Err(fail(
                                                "callee requires(state(...)) argument has unknown protocol handle state",
                                            ));
                                        };
                                        if !st.states[h_idx].is_subset_of(states) {
                                            return Err(fail(
                                                "callee requires(state(...)) precondition not satisfied",
                                            ));
                                        }
                                    }
                                    StateContractClause::RequiresApart { param_a, param_b } => {
                                        let (Some(aa), Some(ab)) = (
                                            args.get(param_a as usize),
                                            args.get(param_b as usize),
                                        ) else {
                                            return Err(fail(
                                                "callee requires(apart(...)) argument missing",
                                            ));
                                        };
                                        let (Some(ha), Some(hb)) =
                                            (st.resolve_handle(aa), st.resolve_handle(ab))
                                        else {
                                            return Err(fail(
                                                "callee requires(apart(...)) argument identity cannot be proved distinct",
                                            ));
                                        };
                                        if ha == hb {
                                            return Err(fail(
                                                "callee requires(apart(...)) violated: aliasing protocol handles",
                                            ));
                                        }
                                        // Two distinct parameters can only be assumed disjoint if
                                        // at least one is local-owned, or exclusive (&mut / by-value),
                                        // or explicitly covered by the caller's own `requires(apart(...))`.
                                        let is_shared_param = |idx: usize| -> bool {
                                            idx < param_count
                                                && matches!(
                                                    tys.get(func.body.locals[idx].ty),
                                                    Some(
                                                        TyKind::Ref {
                                                            mutability: Mutability::Shared,
                                                            ..
                                                        } | TyKind::Desc(_)
                                                    )
                                                )
                                        };
                                        if is_shared_param(ha) && is_shared_param(hb) {
                                            let caller_proves_apart =
                                                func.sig.state_contracts.iter().any(|cc| {
                                                    matches!(
                                                        *cc,
                                                        StateContractClause::RequiresApart {
                                                            param_a: ca,
                                                            param_b: cb,
                                                        } if (ca as usize == ha && cb as usize == hb)
                                                            || (ca as usize == hb && cb as usize == ha)
                                                    )
                                                });
                                            if !caller_proves_apart {
                                                return Err(fail(
                                                    "callee requires(apart(...)) violated: shared parameters may alias",
                                                ));
                                            }
                                        }
                                    }
                                    StateContractClause::ProvidesRetired { .. }
                                    | StateContractClause::ProvidesState { .. }
                                    | StateContractClause::ProvidesOnPending { .. }
                                    | StateContractClause::ProvidesOnReady { .. }
                                    | StateContractClause::ProvidesOnOk { .. }
                                    | StateContractClause::ProvidesOnErr { .. }
                                    | StateContractClause::InvalidationUpperBound
                                    | StateContractClause::Invalidates { .. }
                                    | StateContractClause::AllowsRollback { .. } => {}
                                }
                            }
                            // 2. Apply postconditions & projection-scoped invalidation upper bounds.
                            let callee_has_inv_upper = callee_fn
                                .sig
                                .state_contracts
                                .contains(&StateContractClause::InvalidationUpperBound);
                            let mut contracted_handles = Vec::new();
                            let mut callee_invalidated_targets: Vec<(usize, ContractFieldPath)> =
                                Vec::new();
                            for c in &callee_fn.sig.state_contracts {
                                match *c {
                                    StateContractClause::RequiresState { .. }
                                    | StateContractClause::RequiresApart { .. }
                                    | StateContractClause::InvalidationUpperBound => {}
                                    StateContractClause::Invalidates { param, path } => {
                                        callee_invalidated_targets.push((param as usize, path));
                                        if let Some(Operand::Copy(p) | Operand::Move(p)) =
                                            args.get(param as usize)
                                        {
                                            for (caller_p, mut caller_path) in
                                                st.resolve_param_places(p)
                                            {
                                                caller_path.extend_from_slice(path.as_slice());
                                                st.invalidated.insert((caller_p, caller_path));
                                            }
                                        }
                                    }
                                    StateContractClause::AllowsRollback { param } => {
                                        if let Some(Operand::Copy(p) | Operand::Move(p)) =
                                            args.get(param as usize)
                                            && p.projections.is_empty()
                                            && let PlaceRoot::Local(al) = p.root
                                            && let Some(ma_set) = st.may_alias.get(&al).cloned()
                                        {
                                            for h_idx in ma_set {
                                                if let Some(Some(info)) = local_info.get(h_idx) {
                                                    st.history[h_idx] =
                                                        ProtocolStateSet::all_variants(
                                                            info.variant_count,
                                                        );
                                                    st.rolled_back[h_idx] = true;
                                                }
                                            }
                                        }
                                    }
                                    StateContractClause::ProvidesRetired { param } => {
                                        if let Some(arg) = args.get(param as usize)
                                            && let Some(h_idx) = st.resolve_handle(arg)
                                        {
                                            st.retired[h_idx] = true;
                                            st.invalidate_handle(h_idx);
                                            contracted_handles.push(h_idx);
                                        }
                                    }
                                    StateContractClause::ProvidesState { param, states } => {
                                        if let Some(arg) = args.get(param as usize)
                                            && let Some(h_idx) = st.resolve_handle(arg)
                                        {
                                            st.set_handle_state(h_idx, states);
                                            st.invalidate_handle(h_idx);
                                            contracted_handles.push(h_idx);
                                            if let Operand::Copy(p) | Operand::Move(p) = arg
                                                && p.projections.is_empty()
                                                && let PlaceRoot::Local(al) = p.root
                                                && let Some(ma_set) = st.may_alias.get(&al).cloned()
                                            {
                                                for other_h in ma_set {
                                                    if other_h != h_idx {
                                                        st.weak_update_handle_state(
                                                            other_h, states,
                                                        );
                                                        st.invalidate_handle(other_h);
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    StateContractClause::ProvidesOnPending { param, states } => {
                                        if let Some(arg) = args.get(param as usize)
                                            && let Some(h_idx) = st.resolve_handle(arg)
                                        {
                                            pend_by_handle.insert(h_idx, states);
                                            contracted_handles.push(h_idx);
                                        }
                                    }
                                    StateContractClause::ProvidesOnReady { param, states } => {
                                        if let Some(arg) = args.get(param as usize)
                                            && let Some(h_idx) = st.resolve_handle(arg)
                                        {
                                            ready_by_handle.insert(h_idx, states);
                                            contracted_handles.push(h_idx);
                                        }
                                    }
                                    StateContractClause::ProvidesOnOk { param, states } => {
                                        if let Some(arg) = args.get(param as usize)
                                            && let Some(h_idx) = st.resolve_handle(arg)
                                        {
                                            ok_by_handle.insert(h_idx, states);
                                            contracted_handles.push(h_idx);
                                        }
                                    }
                                    StateContractClause::ProvidesOnErr { param, states } => {
                                        if let Some(arg) = args.get(param as usize)
                                            && let Some(h_idx) = st.resolve_handle(arg)
                                        {
                                            err_by_handle.insert(h_idx, states);
                                            contracted_handles.push(h_idx);
                                        }
                                    }
                                }
                            }
                            if callee_has_inv_upper {
                                // Projection-scoped invalidation: only widen protocol handle state
                                // if an invalidated path overlaps the handle's `state` field (or root).
                                for &(c_param, path) in &callee_invalidated_targets {
                                    if let Some(Operand::Copy(p) | Operand::Move(p)) =
                                        args.get(c_param)
                                        && p.projections.is_empty()
                                        && let PlaceRoot::Local(al) = p.root
                                    {
                                        apply_local_effects(
                                            &mut st,
                                            al,
                                            false,
                                            &contracted_handles,
                                        );
                                        if let Some(ma_set) = st.may_alias.get(&al).cloned() {
                                            for h_idx in ma_set {
                                                if !contracted_handles.contains(&h_idx)
                                                    && let Some(Some(info)) = local_info.get(h_idx)
                                                {
                                                    let touches_state = match info.state_field {
                                                        None => true,
                                                        Some((_, sf)) => path.overlaps(&[sf]),
                                                    };
                                                    if touches_state {
                                                        st.set_handle_state(
                                                            h_idx,
                                                            ProtocolStateSet::all_variants(
                                                                info.variant_count,
                                                            ),
                                                        );
                                                        st.invalidate_handle(h_idx);
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            } else {
                                // Callee has no closed invalidation upper bound: conservatively
                                // apply effects to all &mut/Desc/capability-bearing arguments
                                // while preserving explicitly contracted handle postconditions.
                                apply_all_args(&mut st, &contracted_handles);
                            }
                            if let Some(dl) = dest_local {
                                for (&h_idx, &pend) in &pend_by_handle {
                                    let ready = ready_by_handle
                                        .get(&h_idx)
                                        .copied()
                                        .unwrap_or(st.states[h_idx]);
                                    st.poll_of.insert(dl, (h_idx, pend, ready));
                                }
                                for (&h_idx, &ready) in &ready_by_handle {
                                    if !pend_by_handle.contains_key(&h_idx) {
                                        st.poll_of.insert(dl, (h_idx, st.states[h_idx], ready));
                                    }
                                }
                                if !ok_by_handle.is_empty() || !err_by_handle.is_empty() {
                                    let mut entries = Vec::new();
                                    let mut seen = Vec::new();
                                    for (&h_idx, &ok_st) in &ok_by_handle {
                                        let err_st = err_by_handle.get(&h_idx).copied();
                                        let joined = err_st
                                            .map_or(st.states[h_idx].union(ok_st), |e| {
                                                ok_st.union(e)
                                            });
                                        st.set_handle_state(h_idx, joined);
                                        st.invalidate_handle(h_idx);
                                        entries.push((h_idx, Some(ok_st), err_st));
                                        seen.push(h_idx);
                                    }
                                    for (&h_idx, &err_st) in &err_by_handle {
                                        if !seen.contains(&h_idx) {
                                            let joined = st.states[h_idx].union(err_st);
                                            st.set_handle_state(h_idx, joined);
                                            st.invalidate_handle(h_idx);
                                            entries.push((h_idx, None, Some(err_st)));
                                        }
                                    }
                                    st.eu_of.insert(dl, entries);
                                }
                            }
                        } else {
                            apply_all_args(&mut st, &[]);
                        }
                    }
                    Callee::Closure { closure, .. } => {
                        apply_place_effects(&mut st, closure, true, &[]);
                        apply_all_args(&mut st, &[]);
                    }
                    Callee::DynSlot { object, slot } => {
                        let slot_is_mut = match object {
                            Operand::Copy(p) | Operand::Move(p) if p.projections.is_empty() => {
                                if let PlaceRoot::Local(ol) = p.root
                                    && let Some(ld) = func.body.locals.get(ol.index())
                                    && let Some(TyKind::Ref { pointee, .. }) = tys.get(ld.ty)
                                    && let Some(
                                        TyKind::Dyn(dyn_id) | TyKind::OwnedDyn { def: dyn_id, .. },
                                    ) = tys.get(*pointee)
                                    && let Some(def) = tys.get_dyn(*dyn_id)
                                    && let Some(sig) = def.slots.get(*slot as usize)
                                {
                                    !matches!(sig.receiver, Some(Mutability::Shared))
                                } else {
                                    true
                                }
                            }
                            Operand::Copy(_)
                            | Operand::Move(_)
                            | Operand::Const(_)
                            | Operand::Pooled(_) => true,
                        };
                        apply_operand_effects(&mut st, object, slot_is_mut, &[]);
                        apply_all_args(&mut st, &[]);
                    }
                    Callee::Intrinsic(_) | Callee::Indirect(_) | Callee::Foreign(_) => {
                        apply_all_args(&mut st, &[]);
                    }
                }
                enqueue(*target, st, &mut in_states, &mut queue);
            }
            Terminator::Return { value } => {
                let ret_local = match value {
                    Operand::Copy(p) | Operand::Move(p) if p.projections.is_empty() => {
                        match p.root {
                            PlaceRoot::Local(l) => Some(l),
                            PlaceRoot::Capture(_) => None,
                        }
                    }
                    Operand::Copy(_)
                    | Operand::Move(_)
                    | Operand::Const(_)
                    | Operand::Pooled(_) => None,
                };
                for clause in &func.sig.state_contracts {
                    match *clause {
                        StateContractClause::RequiresState { .. }
                        | StateContractClause::RequiresApart { .. }
                        | StateContractClause::InvalidationUpperBound
                        | StateContractClause::Invalidates { .. }
                        | StateContractClause::AllowsRollback { .. } => {}
                        StateContractClause::ProvidesRetired { param } => {
                            if !st.retired[param as usize] {
                                return Err(fail(
                                    "function fails to satisfy provides(retired(...)) on all return paths",
                                ));
                            }
                        }
                        StateContractClause::ProvidesState { param, states } => {
                            if !st.states[param as usize].is_subset_of(states) {
                                return Err(fail(
                                    "function fails to satisfy provides(state(...)) on all return paths",
                                ));
                            }
                        }
                        StateContractClause::ProvidesOnPending { param, states } => {
                            let p_idx = param as usize;
                            let ok = if let Some(rl) = ret_local
                                && let Some(&(po_p, pend_st, _)) = st.poll_of.get(&rl)
                                && po_p == p_idx
                            {
                                pend_st.is_subset_of(states)
                            } else if let Some(rl) = ret_local
                                && st.enum_variant.get(&rl) == Some(&1)
                            {
                                // Returning Poll.Ready directly; on_pending vacuously holds.
                                true
                            } else {
                                st.states[p_idx].is_subset_of(states)
                            };
                            if !ok {
                                return Err(fail(
                                    "function fails to satisfy provides(on_pending(state(...)))",
                                ));
                            }
                        }
                        StateContractClause::ProvidesOnReady { param, states } => {
                            let p_idx = param as usize;
                            let ok = if let Some(rl) = ret_local
                                && let Some(&(po_p, _, ready_st)) = st.poll_of.get(&rl)
                                && po_p == p_idx
                            {
                                ready_st.is_subset_of(states)
                            } else if let Some(rl) = ret_local
                                && st.enum_variant.get(&rl) == Some(&0)
                            {
                                // Returning Poll.Pending directly; on_ready vacuously holds.
                                true
                            } else {
                                st.states[p_idx].is_subset_of(states)
                            };
                            if !ok {
                                return Err(fail(
                                    "function fails to satisfy provides(on_ready(state(...)))",
                                ));
                            }
                        }
                        StateContractClause::ProvidesOnOk { param, states } => {
                            let p_idx = param as usize;
                            let ok = if let Some(rl) = ret_local
                                && let Some(entries) = st.eu_of.get(&rl)
                                && let Some(&(_, Some(ok_st), _)) =
                                    entries.iter().find(|(h, _, _)| *h == p_idx)
                            {
                                ok_st.is_subset_of(states)
                            } else if let Some(rl) = ret_local
                                && st.error_arm_const.get(&rl) == Some(&false)
                            {
                                // Returning .err(...) directly; on_ok vacuously holds.
                                true
                            } else {
                                st.states[p_idx].is_subset_of(states)
                            };
                            if !ok {
                                return Err(fail(
                                    "function fails to satisfy provides(on_ok(state(...)))",
                                ));
                            }
                        }
                        StateContractClause::ProvidesOnErr { param, states } => {
                            let p_idx = param as usize;
                            let ok = if let Some(rl) = ret_local
                                && let Some(entries) = st.eu_of.get(&rl)
                                && let Some(&(_, _, Some(err_st))) =
                                    entries.iter().find(|(h, _, _)| *h == p_idx)
                            {
                                err_st.is_subset_of(states)
                            } else if let Some(rl) = ret_local
                                && st.error_arm_const.get(&rl) == Some(&true)
                            {
                                // Returning .ok(...) directly; on_err vacuously holds.
                                true
                            } else {
                                st.states[p_idx].is_subset_of(states)
                            };
                            if !ok {
                                return Err(fail(
                                    "function fails to satisfy provides(on_err(state(...)))",
                                ));
                            }
                        }
                    }
                }
                // Verify declared exported projection-invalidation upper bound (`actual ⊆ upper`).
                if has_invalidation_upper {
                    for (p_idx, actual_path) in &st.invalidated {
                        let covered = declared_invalidations
                            .iter()
                            .any(|(dp, dpath)| *dp == *p_idx && dpath.subsumes(actual_path));
                        if !covered {
                            return Err(fail(
                                "function invalidates projection outside declared effects(invalidates(...)) upper bound",
                            ));
                        }
                    }
                }
                // Verify history rollback `H`: a function cannot hide a transient state
                // rollback (e.g. Open -> Closed -> Open) unless `effects(rollback(p))`
                // or an invalidation upper bound covering the handle's state is declared.
                for (p_idx, info_opt) in local_info.iter().enumerate().take(param_count) {
                    let Some(info) = info_opt else { continue };
                    if allows_rollback[p_idx] {
                        continue;
                    }
                    let allowed_history = entry_init_states[p_idx].union(st.states[p_idx]);
                    let has_hidden_rollback =
                        st.rolled_back[p_idx] || !st.history[p_idx].is_subset_of(allowed_history);
                    if has_hidden_rollback
                        && (declared_requires_state[p_idx].is_some() || has_invalidation_upper)
                    {
                        let state_covered = has_invalidation_upper
                            && declared_invalidations.iter().any(|(dp, dpath)| {
                                *dp == p_idx
                                    && match info.state_field {
                                        None => dpath.as_slice().is_empty(),
                                        Some((_, sf)) => dpath.subsumes(&[sf]),
                                    }
                            });
                        if !state_covered {
                            return Err(fail(
                                "hidden protocol state rollback H detected; declare effects(rollback(...)) or effects(invalidates(...))",
                            ));
                        }
                    }
                }
            }
            Terminator::Unreachable | Terminator::Trap(_) => {}
        }
    }
    Ok(())
}

/// Computes transitive `Suspend` (`TaskYield`) effect summaries over the call-graph
/// SCCs and enforces execution-context and synchronous-contract boundaries:
/// - Suspending functions cannot escape via `Rvalue::FnRef` except directly into
///   `TaskCreate` / `TaskStart` / `AsyncRegistrationStart` worker entry slots;
/// - `ThreadScope` callbacks cannot suspend (bare OS thread has no Task frame);
/// - `FnRole::DynSlotImpl`, `FnRole::Drop`, and entry `main` cannot transitively suspend.
fn check_suspend_effects(module: &Module, sccs: &[CallScc]) -> Result<Vec<bool>, CheckError> {
    use std::collections::HashMap;
    use zil_ir::body::{
        Callee, ClosureId, InstructionKind, IntrinsicOp, Local, Operand, Place, Rvalue, Terminator,
    };

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

    struct BodyPlaceTypes<'a> {
        body: &'a zil_ir::body::Body,
        tys: &'a zil_ir::body::TyTable,
    }
    impl zil_ir::body::PlaceTypes for BodyPlaceTypes<'_> {
        fn local_ty(&self, local: Local) -> Option<zil_ir::body::TyId> {
            self.body.locals.get(local.index()).map(|d| d.ty)
        }
        fn capture_ty(&self, index: u32) -> Option<zil_ir::body::TyId> {
            let cid = self.body.closure?;
            self.tys.closure(cid)?.captures.get(index as usize).copied()
        }
        fn tys(&self) -> &zil_ir::body::TyTable {
            self.tys
        }
    }

    let closure_fn_of_place = |body: &zil_ir::body::Body, place: &Place| -> Option<FnId> {
        let cx = BodyPlaceTypes {
            body,
            tys: &module.tys,
        };
        let mut ty = zil_ir::body::type_place(place, &cx).ok()?.ty();
        for _ in 0..4 {
            match module.tys.get(ty)? {
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

    let mut drop_fn_by_adt: HashMap<AdtId, FnId> = HashMap::new();
    for (i, func) in module.functions.iter().enumerate() {
        if let FnRole::Drop(adt) = func.sig.role {
            drop_fn_by_adt.insert(adt, FnId::from_index(i));
        }
    }

    fn ty_can_suspend_on_drop(
        tys: &zil_ir::body::TyTable,
        ty: zil_ir::body::TyId,
        drop_fn_by_adt: &HashMap<AdtId, FnId>,
        can_suspend: &[bool],
        seen: &mut Vec<zil_ir::body::TyId>,
    ) -> bool {
        if seen.contains(&ty) {
            return false;
        }
        seen.push(ty);
        let Some(kind) = tys.get(ty) else {
            return false;
        };
        match kind {
            TyKind::Adt { adt, .. } => {
                if let Some(&dfn) = drop_fn_by_adt.get(adt)
                    && can_suspend[dfn.index()]
                {
                    return true;
                }
                let def = tys.adt(*adt);
                def.variants.iter().any(|v| {
                    v.fields.iter().any(|f| {
                        ty_can_suspend_on_drop(tys, f.ty, drop_fn_by_adt, can_suspend, seen)
                    })
                })
            }
            TyKind::Tuple(elems) => elems
                .iter()
                .any(|&e| ty_can_suspend_on_drop(tys, e, drop_fn_by_adt, can_suspend, seen)),
            TyKind::Array { element, .. } | TyKind::Managed(element) | TyKind::Desc(element) => {
                ty_can_suspend_on_drop(tys, *element, drop_fn_by_adt, can_suspend, seen)
            }
            TyKind::ErrorUnion { ok, err } => {
                ty_can_suspend_on_drop(tys, *ok, drop_fn_by_adt, can_suspend, seen)
                    || ty_can_suspend_on_drop(tys, *err, drop_fn_by_adt, can_suspend, seen)
            }
            TyKind::Closure { def, .. } => tys.closure(*def).is_some_and(|cd| {
                cd.captures
                    .iter()
                    .any(|&c| ty_can_suspend_on_drop(tys, c, drop_fn_by_adt, can_suspend, seen))
            }),
            TyKind::Unit
            | TyKind::Bool
            | TyKind::Int(_)
            | TyKind::Float(_)
            | TyKind::Never
            | TyKind::Ref { .. }
            | TyKind::Slice(_)
            | TyKind::FnPtr { .. }
            | TyKind::Dyn { .. }
            | TyKind::OwnedDyn { .. }
            | TyKind::Opaque(_)
            | TyKind::Param(_) => false,
        }
    }

    // 1. Fixed-point transitive `can_suspend` summary over reverse-topological SCCs,
    //    including `InstructionKind::Drop` propagation for resources with async Drop.
    let mut can_suspend = vec![false; module.functions.len()];
    let mut global_changed = true;
    while global_changed {
        global_changed = false;
        for scc in sccs {
            let mut changed = true;
            while changed {
                changed = false;
                for &fid in &scc.members {
                    if can_suspend[fid.index()] {
                        continue;
                    }
                    let func = &module.functions[fid.index()];
                    let mut suspends = false;
                    for block in &func.body.blocks {
                        for inst in &block.instructions {
                            if let InstructionKind::Drop { place } = &inst.kind
                                && let zil_ir::body::PlaceRoot::Local(local) = place.root
                                && let Some(decl) = func.body.locals.get(local.index())
                            {
                                let mut seen = Vec::new();
                                if ty_can_suspend_on_drop(
                                    &module.tys,
                                    decl.ty,
                                    &drop_fn_by_adt,
                                    &can_suspend,
                                    &mut seen,
                                ) {
                                    suspends = true;
                                    break;
                                }
                            }
                        }
                        if suspends {
                            break;
                        }
                        if let Terminator::Call { callee, .. } = &block.terminator {
                            match callee {
                                Callee::Intrinsic(IntrinsicOp::TaskYield) => {
                                    suspends = true;
                                }
                                Callee::Direct(target) => {
                                    if target.index() < can_suspend.len()
                                        && can_suspend[target.index()]
                                    {
                                        suspends = true;
                                    }
                                }
                                Callee::Closure { closure, .. } => {
                                    match closure_fn_of_place(&func.body, closure) {
                                        Some(cfn) => {
                                            if can_suspend[cfn.index()] {
                                                suspends = true;
                                            }
                                        }
                                        None => {
                                            // Unknown closure target: conservative over-approximation.
                                            suspends = true;
                                        }
                                    }
                                }
                                Callee::Indirect(_)
                                | Callee::DynSlot { .. }
                                | Callee::Foreign(_)
                                | Callee::Intrinsic(_) => {}
                            }
                        }
                        if suspends {
                            break;
                        }
                    }
                    if suspends {
                        can_suspend[fid.index()] = true;
                        changed = true;
                        global_changed = true;
                    }
                }
            }
        }
    }

    // 2. Prevent suspending functions from escaping through `Rvalue::FnRef` into
    //    arbitrary indirect calls or `ThreadScope`, and reject suspending `ThreadScope`
    //    or `RegisterDefer` callbacks.
    for (i, func) in module.functions.iter().enumerate() {
        let fid = FnId::from_index(i);
        let mut suspend_fn_locals: Vec<(Local, usize, usize)> = Vec::new();
        for (bi, block) in func.body.blocks.iter().enumerate() {
            for (ii, inst) in block.instructions.iter().enumerate() {
                if let InstructionKind::Assign {
                    dest,
                    value: Rvalue::FnRef(target),
                } = &inst.kind
                    && target.index() < can_suspend.len()
                    && can_suspend[target.index()]
                {
                    let zil_ir::body::PlaceRoot::Local(root_local) = dest.root else {
                        return Err(fail(
                            fid,
                            "suspending function cannot be stored into a non-local place",
                        ));
                    };
                    if !dest.projections.is_empty() {
                        return Err(fail(
                            fid,
                            "suspending function cannot be stored into a projected place",
                        ));
                    }
                    suspend_fn_locals.push((root_local, bi, ii));
                }
                if let InstructionKind::RegisterDefer {
                    closure: Operand::Copy(p) | Operand::Move(p),
                    ..
                } = &inst.kind
                    && let Some(cfn) = closure_fn_of_place(&func.body, p)
                    && can_suspend[cfn.index()]
                {
                    return Err(fail(
                        fid,
                        "deferred cleanup closure cannot transitively suspend via TaskYield",
                    ));
                }
            }
        }
        for (s_local, def_bi, def_ii) in suspend_fn_locals {
            let s_root = zil_ir::body::PlaceRoot::Local(s_local);
            for (bi, block) in func.body.blocks.iter().enumerate() {
                for (ii, inst) in block.instructions.iter().enumerate() {
                    if bi == def_bi && ii == def_ii {
                        continue;
                    }
                    match &inst.kind {
                        InstructionKind::StorageLive(_)
                        | InstructionKind::StorageDead(_)
                        | InstructionKind::Drop { .. }
                        | InstructionKind::ScopeEnter(_)
                        | InstructionKind::ScopeExit { .. } => {}
                        InstructionKind::Assign { .. }
                        | InstructionKind::Replace { .. }
                        | InstructionKind::RegisterDefer { .. } => {
                            if inst.kind.accesses().iter().any(|a| a.place.root == s_root) {
                                return Err(fail(
                                    fid,
                                    "suspending function cannot escape into non-task function pointer",
                                ));
                            }
                        }
                    }
                }
                let term_uses_local = block
                    .terminator
                    .accesses()
                    .iter()
                    .any(|a| a.place.root == s_root);
                if term_uses_local {
                    let allowed_task_launch = matches!(
                        &block.terminator,
                        Terminator::Call {
                            callee: Callee::Intrinsic(
                                IntrinsicOp::TaskCreate | IntrinsicOp::TaskStart
                            ),
                            args,
                            ..
                        } if matches!(
                            args.first(),
                            Some(Operand::Copy(p) | Operand::Move(p))
                                if p.root == s_root && p.projections.is_empty()
                        ) && args.iter().skip(1).all(|a| !matches!(
                            a,
                            Operand::Copy(p) | Operand::Move(p) if p.root == s_root
                        ))
                    );
                    if !allowed_task_launch {
                        return Err(fail(
                            fid,
                            "suspending function can only be passed as a Task worker entry",
                        ));
                    }
                }
            }
        }

        // Check `ThreadScope`, `RegistrationStart`, and `AsyncRegistrationStart` callbacks.
        for block in &func.body.blocks {
            if let Terminator::Call {
                callee: Callee::Intrinsic(op),
                args,
                ..
            } = &block.terminator
            {
                let cb_arg = if matches!(
                    op,
                    IntrinsicOp::ThreadScope | IntrinsicOp::RegistrationStart
                ) {
                    args.get(2)
                } else if *op == IntrinsicOp::AsyncRegistrationStart {
                    args.get(3)
                } else {
                    None
                };
                if let Some(Operand::Copy(p) | Operand::Move(p)) = cb_arg {
                    let resolved_cfn = closure_fn_of_place(&func.body, p);
                    if let Some(cfn) = resolved_cfn
                        && can_suspend[cfn.index()]
                    {
                        return Err(fail(
                            fid,
                            "foreign or thread_scope callback cannot suspend outside a Task context",
                        ));
                    }
                    if matches!(
                        op,
                        IntrinsicOp::RegistrationStart | IntrinsicOp::AsyncRegistrationStart
                    ) {
                        let env_ty = zil_ir::body::type_place(p, &func.body.cx(&module.tys))
                            .ok()
                            .map(|t| t.ty());
                        if let Some(env_ty) = env_ty {
                            let mut seen = Vec::new();
                            if ty_can_suspend_on_drop(
                                &module.tys,
                                env_ty,
                                &drop_fn_by_adt,
                                &can_suspend,
                                &mut seen,
                            ) {
                                return Err(fail(
                                    fid,
                                    "foreign registration environment cannot have a suspending Drop cleanup",
                                ));
                            }
                        }
                    }
                    // Note: `AsyncRegistrationStart` sealed ThreadCallable verification
                    // is enforced path-sensitively in `verify_fn_provenance`, gated on
                    // whether the proven provider roles are threaded (or unknown/mixed).
                }
                if *op == IntrinsicOp::ThreadScope
                    && let Some(Operand::Copy(cell_p) | Operand::Move(cell_p)) = args.get(1)
                    && let Ok(cell_ty_res) =
                        zil_ir::body::type_place(cell_p, &func.body.cx(&module.tys))
                {
                    let cell_ty = cell_ty_res.ty();
                    let mut seen_guard = Vec::new();
                    if ty_contains_mutex_guard(
                        &module.tys,
                        &module.dyn_impls,
                        cell_ty,
                        &mut seen_guard,
                    ) {
                        return Err(fail(
                            fid,
                            "MutexGuard cannot be transferred across threads via thread_scope",
                        ));
                    }
                    let mut seen_handle = Vec::new();
                    if ty_contains_raw_foreign_handle(
                        &module.tys,
                        &module.dyn_impls,
                        cell_ty,
                        &mut seen_handle,
                    ) {
                        return Err(fail(
                            fid,
                            "raw c_handle (ForeignHandle) is not Transfer and cannot cross thread_scope",
                        ));
                    }
                    let mut seen_drop = Vec::new();
                    let mut drop_fns = Vec::new();
                    collect_mutex_payload_drop_fns(
                        &module.tys,
                        cell_ty,
                        &drop_fn_by_adt,
                        &mut seen_drop,
                        &mut drop_fns,
                    );
                    for dfn in drop_fns {
                        if fn_transitively_calls_foreign(module, dfn, &closure_to_fn) {
                            return Err(fail(
                                fid,
                                "Mutex payload with non-thread-safe foreign Drop cannot cross thread_scope",
                            ));
                        }
                    }
                }
            }
        }
    }

    // 3. Enforce synchronous contracts and execution-context eligibility.
    for dimpl in &module.dyn_impls {
        let mut seen = Vec::new();
        if ty_can_suspend_on_drop(
            &module.tys,
            dimpl.concrete_ty,
            &drop_fn_by_adt,
            &can_suspend,
            &mut seen,
        ) {
            let fid = dimpl.methods.first().copied().unwrap_or(FnId(0));
            return Err(fail(
                fid,
                "OwnedDyn concrete type cannot have a suspending Drop cleanup",
            ));
        }
    }
    for (i, func) in module.functions.iter().enumerate() {
        let fid = FnId::from_index(i);
        if !can_suspend[i] {
            continue;
        }
        match func.sig.role {
            FnRole::DynSlotImpl { .. } => {
                return Err(fail(
                    fid,
                    "dyn impl method may transitively suspend via TaskYield",
                ));
            }
            FnRole::Drop(_) => {
                // Async Drop is permitted iff only dropped inside an eligible Task context;
                // `InstructionKind::Drop` propagation above enforces this on every caller.
            }
            FnRole::Normal => {
                if func.name == "main" {
                    return Err(fail(
                        fid,
                        "entry function `main` cannot suspend outside a Task context",
                    ));
                }
            }
        }
    }

    Ok(can_suspend)
}

/// Conservatively verifies the sealed `ThreadCallable` execution profile
/// (`spec/threaded-foreign-registration.md` §Separate callable obligations)
/// over the complete transitive call graph of an `AsyncRegistrationStart`
/// callback.
///
/// Admits checked synchronous scalar/product/capture operations, `ByteBuffer`
/// length/capacity/borrow, `Store` length/borrow, atomic `Sync*` operations,
/// and statically proved `Callee::Direct` / known `Callee::Closure` calls.
/// Conservatively rejects unknown `extern "C"` foreign calls, indirect calls,
/// erased `DynSlot` calls, unproved closures, non-sealed intrinsics, and
/// temporary user `Drop` cleanup executed during cross-thread invocation.
fn verify_sealed_thread_callable(
    module: &Module,
    caller_fn: FnId,
    entry_fn: FnId,
    closure_to_fn: &std::collections::HashMap<zil_ir::body::ClosureId, FnId>,
    drop_fn_by_adt: &std::collections::HashMap<zil_ir::body::AdtId, FnId>,
) -> Result<(), CheckError> {
    use zil_ir::body::{
        Callee, InstructionKind, IntrinsicOp, Local, Operand, Place, Terminator, TyKind,
    };

    let fail = |reason: &'static str| CheckError {
        function: caller_fn,
        kind: CheckErrorKind::CertificateShape(reason),
    };

    fn ty_has_user_drop(
        tys: &zil_ir::body::TyTable,
        ty: zil_ir::body::TyId,
        drop_fn_by_adt: &std::collections::HashMap<zil_ir::body::AdtId, FnId>,
        seen: &mut Vec<zil_ir::body::TyId>,
    ) -> bool {
        if seen.contains(&ty) {
            return false;
        }
        seen.push(ty);
        let Some(kind) = tys.get(ty) else {
            return false;
        };
        match kind {
            TyKind::Adt { adt, .. } => {
                if drop_fn_by_adt.contains_key(adt) {
                    return true;
                }
                let def = tys.adt(*adt);
                if def.user_drop {
                    return true;
                }
                def.variants.iter().any(|v| {
                    v.fields
                        .iter()
                        .any(|f| ty_has_user_drop(tys, f.ty, drop_fn_by_adt, seen))
                })
            }
            TyKind::Managed(inner) | TyKind::Desc(inner) | TyKind::Array { element: inner, .. } => {
                ty_has_user_drop(tys, *inner, drop_fn_by_adt, seen)
            }
            TyKind::ErrorUnion { ok, err } => {
                ty_has_user_drop(tys, *ok, drop_fn_by_adt, seen)
                    || ty_has_user_drop(tys, *err, drop_fn_by_adt, seen)
            }
            TyKind::Tuple(items) => items
                .iter()
                .any(|&it| ty_has_user_drop(tys, it, drop_fn_by_adt, seen)),
            TyKind::Closure { def, .. } => tys.closure(*def).is_some_and(|c| {
                c.captures
                    .iter()
                    .any(|&cap| ty_has_user_drop(tys, cap, drop_fn_by_adt, seen))
            }),
            TyKind::Unit
            | TyKind::Bool
            | TyKind::Int(_)
            | TyKind::Float(_)
            | TyKind::Never
            | TyKind::Ref { .. }
            | TyKind::Slice(_)
            | TyKind::FnPtr { .. }
            | TyKind::Dyn { .. }
            | TyKind::OwnedDyn { .. }
            | TyKind::Opaque(_)
            | TyKind::Param(_) => false,
        }
    }

    struct BodyPlaceTypes<'a> {
        body: &'a zil_ir::body::Body,
        tys: &'a zil_ir::body::TyTable,
    }
    impl zil_ir::body::PlaceTypes for BodyPlaceTypes<'_> {
        fn local_ty(&self, local: Local) -> Option<zil_ir::body::TyId> {
            self.body.locals.get(local.index()).map(|d| d.ty)
        }
        fn capture_ty(&self, index: u32) -> Option<zil_ir::body::TyId> {
            let cid = self.body.closure?;
            self.tys.closure(cid)?.captures.get(index as usize).copied()
        }
        fn tys(&self) -> &zil_ir::body::TyTable {
            self.tys
        }
    }

    let resolve_closure = |body: &zil_ir::body::Body, place: &Place| -> Option<FnId> {
        let cx = BodyPlaceTypes {
            body,
            tys: &module.tys,
        };
        let mut ty = zil_ir::body::type_place(place, &cx).ok()?.ty();
        loop {
            match module.tys.get(ty)? {
                TyKind::Closure { def, .. } => return closure_to_fn.get(def).copied(),
                TyKind::Ref { pointee, .. } => ty = *pointee,
                TyKind::Unit
                | TyKind::Bool
                | TyKind::Never
                | TyKind::Int(_)
                | TyKind::Float(_)
                | TyKind::Adt { .. }
                | TyKind::Tuple(_)
                | TyKind::Array { .. }
                | TyKind::Slice(_)
                | TyKind::Managed(_)
                | TyKind::Desc(_)
                | TyKind::ErrorUnion { .. }
                | TyKind::FnPtr { .. }
                | TyKind::Dyn(_)
                | TyKind::OwnedDyn { .. }
                | TyKind::Param(_)
                | TyKind::Opaque(_) => return None,
            }
        }
    };

    let mut visited = Vec::new();
    let mut worklist = vec![entry_fn];
    while let Some(cur) = worklist.pop() {
        if visited.contains(&cur) {
            continue;
        }
        visited.push(cur);
        let Some(func) = module.functions.get(cur.index()) else {
            return Err(fail("ThreadCallable call graph references invalid FnId"));
        };
        for block in &func.body.blocks {
            for inst in &block.instructions {
                match &inst.kind {
                    InstructionKind::Drop { place } => {
                        if let zil_ir::body::PlaceRoot::Local(local) = place.root
                            && let Some(decl) = func.body.locals.get(local.index())
                        {
                            let mut seen = Vec::new();
                            if ty_has_user_drop(&module.tys, decl.ty, drop_fn_by_adt, &mut seen) {
                                return Err(fail(
                                    "ThreadCallable callback cannot execute user Drop during cross-thread invocation",
                                ));
                            }
                        }
                    }
                    InstructionKind::RegisterDefer {
                        closure: Operand::Copy(dp) | Operand::Move(dp),
                        ..
                    } => {
                        let Some(dfn) = resolve_closure(&func.body, dp) else {
                            return Err(fail(
                                "ThreadCallable callback cannot register unproved defer closure",
                            ));
                        };
                        worklist.push(dfn);
                    }
                    InstructionKind::RegisterDefer {
                        closure: Operand::Const(_) | Operand::Pooled(_),
                        ..
                    } => {
                        return Err(fail(
                            "ThreadCallable callback cannot register constant defer closure",
                        ));
                    }
                    InstructionKind::Assign { .. }
                    | InstructionKind::Replace { .. }
                    | InstructionKind::StorageLive(_)
                    | InstructionKind::StorageDead(_)
                    | InstructionKind::ScopeEnter(_)
                    | InstructionKind::ScopeExit { .. } => {}
                }
            }
            if let Terminator::Call { callee, .. } = &block.terminator {
                match callee {
                    Callee::Direct(target) => worklist.push(*target),
                    Callee::Closure { closure, .. } => {
                        let Some(cfn) = resolve_closure(&func.body, closure) else {
                            return Err(fail(
                                "ThreadCallable callback cannot invoke unproved closure",
                            ));
                        };
                        worklist.push(cfn);
                    }
                    Callee::Foreign(_) => {
                        return Err(fail(
                            "ThreadCallable callback cannot call unknown extern \"C\" function",
                        ));
                    }
                    Callee::Indirect(_) => {
                        return Err(fail(
                            "ThreadCallable callback cannot perform indirect function-pointer call",
                        ));
                    }
                    Callee::DynSlot { .. } => {
                        return Err(fail(
                            "ThreadCallable callback cannot perform erased dyn dispatch",
                        ));
                    }
                    Callee::Intrinsic(op) => match op {
                        IntrinsicOp::ByteBufferLen
                        | IntrinsicOp::ByteBufferCapacity
                        | IntrinsicOp::ByteBufferBorrow
                        | IntrinsicOp::StoreLen
                        | IntrinsicOp::StoreBorrow { .. }
                        | IntrinsicOp::SyncLoad
                        | IntrinsicOp::SyncStore
                        | IntrinsicOp::SyncFetchAdd
                        | IntrinsicOp::SyncCompareExchange
                        | IntrinsicOp::SyncLoadOrdered(_)
                        | IntrinsicOp::SyncStoreOrdered(_)
                        | IntrinsicOp::SyncFetchAddOrdered(_)
                        | IntrinsicOp::SyncExchangeOrdered(_)
                        | IntrinsicOp::SyncCompareExchangeOrdered { .. } => {}
                        IntrinsicOp::StoreNew
                        | IntrinsicOp::StoreReserve
                        | IntrinsicOp::StorePush
                        | IntrinsicOp::StorePop
                        | IntrinsicOp::StoreCapacity
                        | IntrinsicOp::ByteBufferNew
                        | IntrinsicOp::ByteBufferReserve
                        | IntrinsicOp::ByteBufferPush
                        | IntrinsicOp::ByteBufferTruncate
                        | IntrinsicOp::MutexCreate
                        | IntrinsicOp::MutexLock
                        | IntrinsicOp::MutexValue
                        | IntrinsicOp::MutexUnlock
                        | IntrinsicOp::TaskStart
                        | IntrinsicOp::TaskCreate
                        | IntrinsicOp::TaskYield
                        | IntrinsicOp::TaskValid
                        | IntrinsicOp::TaskPoll
                        | IntrinsicOp::TaskRequestCancel
                        | IntrinsicOp::TaskPollClose
                        | IntrinsicOp::TaskFinishDestroy
                        | IntrinsicOp::RegistrationStart
                        | IntrinsicOp::RegistrationValid
                        | IntrinsicOp::RegistrationClose
                        | IntrinsicOp::AsyncRegistrationStart
                        | IntrinsicOp::AsyncRegistrationValid
                        | IntrinsicOp::AsyncRegistrationRequestClose
                        | IntrinsicOp::AsyncRegistrationPollClose
                        | IntrinsicOp::AsyncRegistrationFinishDestroy
                        | IntrinsicOp::ManagedCreate
                        | IntrinsicOp::OwnedDynCreate
                        | IntrinsicOp::ThreadScope
                        | IntrinsicOp::SyncInit
                        | IntrinsicOp::ScopedCallback
                        | IntrinsicOp::CNull
                        | IntrinsicOp::CCallback => {
                            return Err(fail(
                                "ThreadCallable callback cannot invoke non-sealed runtime intrinsic",
                            ));
                        }
                    },
                }
            }
        }
    }
    Ok(())
}

fn ty_contains_mutex_guard(
    tys: &zil_ir::body::TyTable,
    dyn_impls: &[zil_ir::body::DynImpl],
    ty: zil_ir::body::TyId,
    seen: &mut Vec<zil_ir::body::TyId>,
) -> bool {
    use zil_ir::body::OpaqueTy;
    if seen.contains(&ty) {
        return false;
    }
    seen.push(ty);
    match tys.get(ty) {
        Some(TyKind::Opaque(OpaqueTy::MutexGuard { .. })) => true,
        Some(TyKind::Tuple(elems)) => elems
            .iter()
            .any(|e| ty_contains_mutex_guard(tys, dyn_impls, *e, seen)),
        Some(
            TyKind::Array { element: inner, .. }
            | TyKind::Slice(inner)
            | TyKind::Ref { pointee: inner, .. }
            | TyKind::Managed(inner)
            | TyKind::Desc(inner),
        ) => ty_contains_mutex_guard(tys, dyn_impls, *inner, seen),
        Some(TyKind::ErrorUnion { ok, err }) => {
            ty_contains_mutex_guard(tys, dyn_impls, *ok, seen)
                || ty_contains_mutex_guard(tys, dyn_impls, *err, seen)
        }
        Some(TyKind::Adt { adt, args }) => tys.get_adt(*adt).is_some_and(|def| {
            def.variants.iter().any(|v| {
                v.fields.iter().any(|f| {
                    let fty = tys.subst_lookup(f.ty, *args).unwrap_or(f.ty);
                    ty_contains_mutex_guard(tys, dyn_impls, fty, seen)
                })
            })
        }),
        Some(TyKind::Closure { def, args }) => tys.closure(*def).is_some_and(|cdef| {
            cdef.captures.iter().any(|cap| {
                let cty = tys.subst_lookup(*cap, *args).unwrap_or(*cap);
                ty_contains_mutex_guard(tys, dyn_impls, cty, seen)
            })
        }),
        Some(TyKind::Dyn(dyn_id) | TyKind::OwnedDyn { def: dyn_id, .. }) => dyn_impls
            .iter()
            .filter(|di| di.dyn_id == *dyn_id)
            .any(|di| ty_contains_mutex_guard(tys, dyn_impls, di.concrete_ty, seen)),
        Some(
            TyKind::Unit
            | TyKind::Bool
            | TyKind::Never
            | TyKind::Int(_)
            | TyKind::Float(_)
            | TyKind::FnPtr { .. }
            | TyKind::Param(_)
            | TyKind::Opaque(_),
        )
        | None => false,
    }
}

fn ty_contains_raw_foreign_handle(
    tys: &zil_ir::body::TyTable,
    dyn_impls: &[zil_ir::body::DynImpl],
    ty: zil_ir::body::TyId,
    seen: &mut Vec<zil_ir::body::TyId>,
) -> bool {
    use zil_ir::body::OpaqueTy;
    if seen.contains(&ty) {
        return false;
    }
    seen.push(ty);
    match tys.get(ty) {
        Some(TyKind::Opaque(OpaqueTy::ForeignHandle)) => true,
        Some(TyKind::Opaque(OpaqueTy::Mutex { payload })) => {
            ty_contains_raw_foreign_handle(tys, dyn_impls, *payload, seen)
        }
        Some(TyKind::Tuple(elems)) => elems
            .iter()
            .any(|e| ty_contains_raw_foreign_handle(tys, dyn_impls, *e, seen)),
        Some(
            TyKind::Array { element: inner, .. }
            | TyKind::Slice(inner)
            | TyKind::Ref { pointee: inner, .. }
            | TyKind::Managed(inner)
            | TyKind::Desc(inner),
        ) => ty_contains_raw_foreign_handle(tys, dyn_impls, *inner, seen),
        Some(TyKind::ErrorUnion { ok, err }) => {
            ty_contains_raw_foreign_handle(tys, dyn_impls, *ok, seen)
                || ty_contains_raw_foreign_handle(tys, dyn_impls, *err, seen)
        }
        Some(TyKind::Adt { adt, args }) => tys.get_adt(*adt).is_some_and(|def| {
            def.variants.iter().any(|v| {
                v.fields.iter().any(|f| {
                    let fty = tys.subst_lookup(f.ty, *args).unwrap_or(f.ty);
                    ty_contains_raw_foreign_handle(tys, dyn_impls, fty, seen)
                })
            })
        }),
        Some(TyKind::Closure { def, args }) => tys.closure(*def).is_some_and(|cdef| {
            cdef.captures.iter().any(|cap| {
                let cty = tys.subst_lookup(*cap, *args).unwrap_or(*cap);
                ty_contains_raw_foreign_handle(tys, dyn_impls, cty, seen)
            })
        }),
        Some(TyKind::Dyn(dyn_id) | TyKind::OwnedDyn { def: dyn_id, .. }) => dyn_impls
            .iter()
            .filter(|di| di.dyn_id == *dyn_id)
            .any(|di| ty_contains_raw_foreign_handle(tys, dyn_impls, di.concrete_ty, seen)),
        Some(
            TyKind::Unit
            | TyKind::Bool
            | TyKind::Never
            | TyKind::Int(_)
            | TyKind::Float(_)
            | TyKind::FnPtr { .. }
            | TyKind::Param(_)
            | TyKind::Opaque(_),
        )
        | None => false,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Prov<T> {
    Bot,
    Exact(T),
    Top,
}

impl<T: Copy + Eq> Prov<T> {
    fn join(self, other: Self) -> Self {
        match (self, other) {
            (Self::Bot, x) | (x, Self::Bot) => x,
            (Self::Exact(a), Self::Exact(b)) if a == b => Self::Exact(a),
            (Self::Exact(_), Self::Exact(_))
            | (Self::Top, Self::Exact(_) | Self::Top)
            | (Self::Exact(_), Self::Top) => Self::Top,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ProvState {
    fn_ref: Vec<Prov<FnId>>,
    foreign_ref: Vec<Prov<zil_ir::body::ForeignId>>,
    task_transfer: Vec<Prov<bool>>,
}

impl ProvState {
    fn bottom(n: usize) -> Self {
        Self {
            fn_ref: vec![Prov::Bot; n],
            foreign_ref: vec![Prov::Bot; n],
            task_transfer: vec![Prov::Bot; n],
        }
    }

    fn entry(n: usize, param_count: usize) -> Self {
        let mut s = Self::bottom(n);
        for i in 0..param_count.min(n) {
            s.fn_ref[i] = Prov::Top;
            s.foreign_ref[i] = Prov::Top;
            s.task_transfer[i] = Prov::Top;
        }
        s
    }

    fn kill(&mut self, l: zil_ir::body::Local) {
        let i = l.index();
        if i < self.fn_ref.len() {
            self.fn_ref[i] = Prov::Top;
            self.foreign_ref[i] = Prov::Top;
            self.task_transfer[i] = Prov::Top;
        }
    }

    fn clear(&mut self, l: zil_ir::body::Local) {
        let i = l.index();
        if i < self.fn_ref.len() {
            self.fn_ref[i] = Prov::Bot;
            self.foreign_ref[i] = Prov::Bot;
            self.task_transfer[i] = Prov::Bot;
        }
    }

    fn join_from(&mut self, other: &Self) -> bool {
        let mut changed = false;
        for i in 0..self.fn_ref.len() {
            let nf = self.fn_ref[i].join(other.fn_ref[i]);
            if nf != self.fn_ref[i] {
                self.fn_ref[i] = nf;
                changed = true;
            }
            let nfor = self.foreign_ref[i].join(other.foreign_ref[i]);
            if nfor != self.foreign_ref[i] {
                self.foreign_ref[i] = nfor;
                changed = true;
            }
            let nt = self.task_transfer[i].join(other.task_transfer[i]);
            if nt != self.task_transfer[i] {
                self.task_transfer[i] = nt;
                changed = true;
            }
        }
        changed
    }

    fn exact_fn(&self, op: &zil_ir::body::Operand) -> Option<FnId> {
        use zil_ir::body::{Operand, PlaceRoot};
        let (Operand::Copy(p) | Operand::Move(p)) = op else {
            return None;
        };
        if !p.projections.is_empty() {
            return None;
        }
        let PlaceRoot::Local(l) = p.root else {
            return None;
        };
        match self.fn_ref.get(l.index()).copied() {
            Some(Prov::Exact(fid)) => Some(fid),
            Some(Prov::Bot | Prov::Top) | None => None,
        }
    }

    fn exact_foreign(&self, op: &zil_ir::body::Operand) -> Option<zil_ir::body::ForeignId> {
        use zil_ir::body::{Operand, PlaceRoot};
        let (Operand::Copy(p) | Operand::Move(p)) = op else {
            return None;
        };
        if !p.projections.is_empty() {
            return None;
        }
        let PlaceRoot::Local(l) = p.root else {
            return None;
        };
        match self.foreign_ref.get(l.index()).copied() {
            Some(Prov::Exact(fid)) => Some(fid),
            Some(Prov::Bot | Prov::Top) | None => None,
        }
    }

    fn is_proved_transfer_task(&self, op: &zil_ir::body::Operand) -> bool {
        use zil_ir::body::{Operand, PlaceRoot};
        let (Operand::Copy(p) | Operand::Move(p)) = op else {
            return false;
        };
        if !p.projections.is_empty() {
            return false;
        }
        let PlaceRoot::Local(l) = p.root else {
            return false;
        };
        matches!(self.task_transfer.get(l.index()), Some(Prov::Exact(true)))
    }
}

/// Identifies locals that are solely produced by `&mut place` and consumed
/// exclusively as direct arguments to `TaskPoll`, `TaskRequestCancel`, or
/// `TaskPollClose`. Because those intrinsics never replace the underlying
/// task's worker function, taking such an exclusive borrow preserves the
/// borrowed local's `task_transfer` provenance; every other `&mut` borrow
/// conservatively kills provenance.
fn compute_task_ops_only_refs(body: &zil_ir::body::Body) -> Vec<bool> {
    use zil_ir::body::{
        Callee, InstructionKind, IntrinsicOp, Mutability, Operand, Place, PlaceRoot, Rvalue,
        Terminator,
    };

    let n = body.locals.len();
    let mut eligible = vec![true; n];
    for slot in eligible.iter_mut().take(body.param_count as usize) {
        *slot = false;
    }
    let disqualify_place = |p: &Place, eligible: &mut [bool]| {
        if let PlaceRoot::Local(l) = p.root
            && let Some(slot) = eligible.get_mut(l.index())
        {
            *slot = false;
        }
    };
    let disqualify_op = |op: &Operand, eligible: &mut [bool]| match op {
        Operand::Copy(p) | Operand::Move(p) => disqualify_place(p, eligible),
        Operand::Const(_) | Operand::Pooled(_) => {}
    };

    for block in &body.blocks {
        for inst in &block.instructions {
            match &inst.kind {
                InstructionKind::Assign { dest, value } => {
                    let is_direct_mut_ref = dest.projections.is_empty()
                        && matches!(
                            value,
                            Rvalue::Ref {
                                mutability: Mutability::Exclusive,
                                ..
                            }
                        );
                    if !is_direct_mut_ref {
                        disqualify_place(dest, &mut eligible);
                    }
                    match value {
                        Rvalue::Use(op)
                        | Rvalue::Unary { operand: op, .. }
                        | Rvalue::Convert { operand: op, .. }
                        | Rvalue::ErrorUnion { value: op, .. } => {
                            disqualify_op(op, &mut eligible);
                        }
                        Rvalue::Binary { lhs, rhs, .. } => {
                            disqualify_op(lhs, &mut eligible);
                            disqualify_op(rhs, &mut eligible);
                        }
                        Rvalue::Aggregate { operands, .. } => {
                            for f in operands {
                                disqualify_op(f, &mut eligible);
                            }
                        }
                        Rvalue::Ref { place, .. }
                        | Rvalue::SliceRef { place, .. }
                        | Rvalue::Describe { place }
                        | Rvalue::Len(place)
                        | Rvalue::Discriminant(place)
                        | Rvalue::ErrorArm(place) => {
                            disqualify_place(place, &mut eligible);
                        }
                        Rvalue::FnRef(_) | Rvalue::ForeignFnRef(_) | Rvalue::StaticUtf(_) => {}
                    }
                }
                InstructionKind::Replace { place, value, old } => {
                    disqualify_place(place, &mut eligible);
                    disqualify_op(value, &mut eligible);
                    if let Some(slot) = eligible.get_mut(old.index()) {
                        *slot = false;
                    }
                }
                InstructionKind::Drop { place } => {
                    if !place.projections.is_empty() {
                        disqualify_place(place, &mut eligible);
                    }
                }
                InstructionKind::StorageLive(_)
                | InstructionKind::StorageDead(_)
                | InstructionKind::ScopeEnter(_)
                | InstructionKind::ScopeExit { .. }
                | InstructionKind::RegisterDefer { .. } => {}
            }
        }
        match &block.terminator {
            Terminator::Call {
                callee, args, dest, ..
            } => {
                disqualify_place(dest, &mut eligible);
                let is_task_state_op = matches!(
                    callee,
                    Callee::Intrinsic(
                        IntrinsicOp::TaskPoll
                            | IntrinsicOp::TaskRequestCancel
                            | IntrinsicOp::TaskPollClose
                    )
                );
                match callee {
                    Callee::Indirect(op) | Callee::DynSlot { object: op, .. } => {
                        disqualify_op(op, &mut eligible);
                    }
                    Callee::Closure { closure, .. } => {
                        disqualify_place(closure, &mut eligible);
                    }
                    Callee::Direct(_) | Callee::Foreign(_) | Callee::Intrinsic(_) => {}
                }
                for arg in args {
                    if is_task_state_op
                        && matches!(
                            arg,
                            Operand::Copy(p) | Operand::Move(p)
                            if p.projections.is_empty() && matches!(p.root, PlaceRoot::Local(_))
                        )
                    {
                        continue;
                    }
                    disqualify_op(arg, &mut eligible);
                }
            }
            Terminator::Return { value } => {
                disqualify_op(value, &mut eligible);
            }
            Terminator::Assert { cond, .. } => {
                disqualify_op(cond, &mut eligible);
            }
            Terminator::Switch { discr, .. } => {
                disqualify_op(discr, &mut eligible);
            }
            Terminator::Goto { .. } | Terminator::Unreachable | Terminator::Trap(_) => {}
        }
    }
    eligible
}

fn step_provenance_inst(
    state: &mut ProvState,
    kind: &zil_ir::body::InstructionKind,
    task_ops_only_refs: &[bool],
) {
    use zil_ir::body::{InstructionKind, Mutability, Operand, PlaceRoot, Projection, Rvalue};
    match kind {
        InstructionKind::Assign { dest, value } => {
            if let Rvalue::Ref {
                mutability: Mutability::Exclusive,
                place,
                ..
            } = value
                && let PlaceRoot::Local(rl) = place.root
            {
                let preserves_task_identity = dest.projections.is_empty()
                    && matches!(
                        dest.root,
                        PlaceRoot::Local(dl)
                        if task_ops_only_refs.get(dl.index()).copied().unwrap_or(false)
                    );
                if !preserves_task_identity {
                    state.kill(rl);
                }
            }
            let PlaceRoot::Local(dl) = dest.root else {
                return;
            };
            if !dest.projections.is_empty() {
                state.kill(dl);
                return;
            }
            let di = dl.index();
            match value {
                Rvalue::FnRef(fid) => {
                    state.fn_ref[di] = Prov::Exact(*fid);
                    state.foreign_ref[di] = Prov::Top;
                    state.task_transfer[di] = Prov::Top;
                }
                Rvalue::ForeignFnRef(for_id) => {
                    state.fn_ref[di] = Prov::Top;
                    state.foreign_ref[di] = Prov::Exact(*for_id);
                    state.task_transfer[di] = Prov::Top;
                }
                Rvalue::Use(Operand::Copy(src) | Operand::Move(src)) => {
                    if let PlaceRoot::Local(sl) = src.root {
                        let si = sl.index();
                        if src.projections.is_empty() {
                            state.fn_ref[di] = state.fn_ref[si];
                            state.foreign_ref[di] = state.foreign_ref[si];
                            state.task_transfer[di] = state.task_transfer[si];
                        } else if matches!(src.projections.as_slice(), [Projection::ErrorArm(true)])
                        {
                            state.fn_ref[di] = Prov::Top;
                            state.foreign_ref[di] = Prov::Top;
                            state.task_transfer[di] = state.task_transfer[si];
                        } else {
                            state.kill(dl);
                        }
                    } else {
                        state.kill(dl);
                    }
                }
                Rvalue::ErrorUnion {
                    ok: true,
                    value: Operand::Copy(src) | Operand::Move(src),
                    ..
                } => {
                    if let PlaceRoot::Local(sl) = src.root
                        && src.projections.is_empty()
                    {
                        state.fn_ref[di] = Prov::Top;
                        state.foreign_ref[di] = Prov::Top;
                        state.task_transfer[di] = state.task_transfer[sl.index()];
                    } else {
                        state.kill(dl);
                    }
                }
                Rvalue::Use(_)
                | Rvalue::ErrorUnion { .. }
                | Rvalue::Ref { .. }
                | Rvalue::SliceRef { .. }
                | Rvalue::Describe { .. }
                | Rvalue::Aggregate { .. }
                | Rvalue::Unary { .. }
                | Rvalue::Binary { .. }
                | Rvalue::Convert { .. }
                | Rvalue::Len(_)
                | Rvalue::Discriminant(_)
                | Rvalue::ErrorArm(_)
                | Rvalue::StaticUtf(_) => state.kill(dl),
            }
        }
        InstructionKind::Replace { place, value, old } => {
            let oi = old.index();
            if place.projections.is_empty()
                && let PlaceRoot::Local(pl) = place.root
            {
                let pi = pl.index();
                state.fn_ref[oi] = state.fn_ref[pi];
                state.foreign_ref[oi] = state.foreign_ref[pi];
                state.task_transfer[oi] = state.task_transfer[pi];
                if let Operand::Copy(src) | Operand::Move(src) = value
                    && src.projections.is_empty()
                    && let PlaceRoot::Local(sl) = src.root
                {
                    let si = sl.index();
                    state.fn_ref[pi] = state.fn_ref[si];
                    state.foreign_ref[pi] = state.foreign_ref[si];
                    state.task_transfer[pi] = state.task_transfer[si];
                } else {
                    state.kill(pl);
                }
            } else {
                state.kill(*old);
                if let PlaceRoot::Local(pl) = place.root {
                    state.kill(pl);
                }
            }
        }
        InstructionKind::StorageLive(l) => state.clear(*l),
        InstructionKind::StorageDead(l) => state.kill(*l),
        InstructionKind::Drop { place } => {
            if let PlaceRoot::Local(l) = place.root {
                state.kill(l);
            }
        }
        InstructionKind::ScopeEnter(_)
        | InstructionKind::ScopeExit { .. }
        | InstructionKind::RegisterDefer { .. } => {}
    }
}

fn verify_fn_provenance(
    module: &Module,
    fn_id: FnId,
    func: &zil_ir::body::Function,
    transfer_reachable: bool,
) -> Result<(), CheckError> {
    use zil_ir::body::{
        Callee, ForeignCallbackRole, IntrinsicOp, OpaqueTy, Operand, PlaceRoot, ResumeMode,
        Terminator,
    };

    let n_blocks = func.body.blocks.len();
    if n_blocks == 0 {
        return Ok(());
    }
    let n_locals = func.body.locals.len();
    let fail = |reason: &'static str| CheckError {
        function: fn_id,
        kind: CheckErrorKind::CertificateShape(reason),
    };

    let task_ops_only_refs = compute_task_ops_only_refs(&func.body);

    // 1. Forward fixed-point lattice propagation over the CFG.
    let mut in_states: Vec<Option<ProvState>> = vec![None; n_blocks];
    in_states[0] = Some(ProvState::entry(n_locals, func.body.param_count as usize));
    let mut worklist: std::collections::VecDeque<usize> = std::collections::VecDeque::new();
    worklist.push_back(0);

    let propagate = |succ: usize,
                     out: &ProvState,
                     in_states: &mut [Option<ProvState>],
                     worklist: &mut std::collections::VecDeque<usize>| {
        if succ >= n_blocks {
            return;
        }
        match &mut in_states[succ] {
            None => {
                in_states[succ] = Some(out.clone());
                worklist.push_back(succ);
            }
            Some(existing) => {
                if existing.join_from(out) {
                    worklist.push_back(succ);
                }
            }
        }
    };

    while let Some(bi) = worklist.pop_front() {
        let Some(mut state) = in_states[bi].clone() else {
            continue;
        };
        let block = &func.body.blocks[bi];
        for inst in &block.instructions {
            step_provenance_inst(&mut state, &inst.kind, &task_ops_only_refs);
        }
        match &block.terminator {
            Terminator::Goto { target } => {
                propagate(target.index(), &state, &mut in_states, &mut worklist);
            }
            Terminator::Switch {
                arms, otherwise, ..
            } => {
                for (_, t) in arms {
                    propagate(t.index(), &state, &mut in_states, &mut worklist);
                }
                propagate(otherwise.index(), &state, &mut in_states, &mut worklist);
            }
            Terminator::Assert { target, .. } => {
                propagate(target.index(), &state, &mut in_states, &mut worklist);
            }
            Terminator::Call {
                callee,
                args,
                dest,
                target,
                cancel,
            } => {
                let mut normal_out = state.clone();
                if let PlaceRoot::Local(dl) = dest.root {
                    if dest.projections.is_empty()
                        && matches!(callee, Callee::Intrinsic(IntrinsicOp::TaskCreate))
                    {
                        let transfer_ok = args
                            .first()
                            .and_then(|a| state.exact_fn(a))
                            .and_then(|wf| module.functions.get(wf.index()))
                            .is_some_and(|wf| wf.sig.resume_mode == ResumeMode::Transfer);
                        let di = dl.index();
                        normal_out.fn_ref[di] = Prov::Top;
                        normal_out.foreign_ref[di] = Prov::Top;
                        normal_out.task_transfer[di] = if transfer_ok {
                            Prov::Exact(true)
                        } else {
                            Prov::Top
                        };
                    } else {
                        normal_out.kill(dl);
                    }
                }
                propagate(target.index(), &normal_out, &mut in_states, &mut worklist);
                if let Some(c) = cancel {
                    let mut cancel_out = state;
                    if let PlaceRoot::Local(dl) = dest.root {
                        cancel_out.kill(dl);
                    }
                    propagate(c.index(), &cancel_out, &mut in_states, &mut worklist);
                }
            }
            Terminator::Return { .. } | Terminator::Unreachable | Terminator::Trap(_) => {}
        }
    }

    // 2. Verify every reachable call site against the converged per-path provenance state.
    for (bi, block) in func.body.blocks.iter().enumerate() {
        let Some(mut state) = in_states[bi].clone() else {
            continue;
        };
        for inst in &block.instructions {
            step_provenance_inst(&mut state, &inst.kind, &task_ops_only_refs);
        }
        let Terminator::Call { callee, args, .. } = &block.terminator else {
            continue;
        };
        match callee {
            Callee::Foreign(for_id) => {
                if let Some(fdecl) = module.foreigns.get(for_id.index())
                    && matches!(
                        fdecl.callback_role,
                        Some(
                            ForeignCallbackRole::ScopedRegister | ForeignCallbackRole::ScopedClose
                        )
                    )
                {
                    return Err(fail(
                        "scoped callback provider cannot be called directly outside c_with_callback",
                    ));
                }
            }
            Callee::Intrinsic(IntrinsicOp::AsyncRegistrationStart) => {
                let mut family_threaded: Option<bool> = None;
                let mut proved_single_thread_slots = 0usize;
                for (slot_idx, arg) in args.iter().take(3).enumerate() {
                    let Some(fid) = state.exact_foreign(arg) else {
                        if transfer_reachable {
                            return Err(fail(
                                "resume(transfer) requires statically known extern callback provider with thread_async_driver_transfer",
                            ));
                        }
                        continue;
                    };
                    let Some(fdecl) = module.foreigns.get(fid.index()) else {
                        return Err(fail("c_register_async references unknown ForeignId"));
                    };
                    if transfer_reachable {
                        let want_role = match slot_idx {
                            0 => ForeignCallbackRole::ThreadAsyncRegister,
                            1 => ForeignCallbackRole::ThreadAsyncRequestClose,
                            2 => ForeignCallbackRole::ThreadAsyncPollDrain,
                            _ => unreachable!(),
                        };
                        if fdecl.callback_role != Some(want_role)
                            || !fdecl.thread_async_driver_transfer
                        {
                            return Err(fail(
                                "resume(transfer) requires explicit thread_async_callback_* role and thread_async_driver_transfer on all providers",
                            ));
                        }
                    } else if let Some(role) = fdecl.callback_role {
                        let role_ok = match slot_idx {
                            0 => matches!(
                                role,
                                ForeignCallbackRole::AsyncRegister
                                    | ForeignCallbackRole::ThreadAsyncRegister
                            ),
                            1 => matches!(
                                role,
                                ForeignCallbackRole::AsyncRequestClose
                                    | ForeignCallbackRole::ThreadAsyncRequestClose
                            ),
                            2 => matches!(
                                role,
                                ForeignCallbackRole::AsyncPollDrain
                                    | ForeignCallbackRole::ThreadAsyncPollDrain
                            ),
                            _ => true,
                        };
                        if !role_ok {
                            return Err(fail(
                                "c_register_async slot does not match extern callback provider role",
                            ));
                        }
                        let threaded = role.is_threaded();
                        if let Some(prev) = family_threaded {
                            if prev != threaded {
                                return Err(fail(
                                    "c_register_async mixes single-thread and threaded extern callback providers",
                                ));
                            }
                        } else {
                            family_threaded = Some(threaded);
                        }
                        if !threaded {
                            proved_single_thread_slots += 1;
                        }
                    }
                }
                let proved_single_thread = !transfer_reachable
                    && family_threaded == Some(false)
                    && proved_single_thread_slots == 3;
                if !proved_single_thread {
                    let cb_fn = args.get(3).and_then(|arg| match arg {
                        Operand::Copy(p) | Operand::Move(p) => {
                            let ty = zil_ir::body::type_place(p, &func.body.cx(&module.tys))
                                .ok()?
                                .ty();
                            if let TyKind::Closure { def, .. } = module.tys.kind(ty) {
                                module.functions.iter().enumerate().find_map(|(i, f)| {
                                    (f.body.closure == Some(*def)).then_some(FnId::from_index(i))
                                })
                            } else {
                                None
                            }
                        }
                        Operand::Const(_) | Operand::Pooled(_) => None,
                    });
                    let Some(entry_fn) = cb_fn else {
                        return Err(fail(
                            "async_registration.start callback must resolve to a statically proved sealed ThreadCallable closure",
                        ));
                    };
                    let mut closure_to_fn = std::collections::HashMap::new();
                    let mut drop_fn_by_adt = std::collections::HashMap::new();
                    for (i, f) in module.functions.iter().enumerate() {
                        let id = FnId::from_index(i);
                        if let Some(cid) = f.body.closure {
                            closure_to_fn.insert(cid, id);
                        }
                        if let FnRole::Drop(adt) = f.sig.role {
                            drop_fn_by_adt.insert(adt, id);
                        }
                    }
                    verify_sealed_thread_callable(
                        module,
                        fn_id,
                        entry_fn,
                        &closure_to_fn,
                        &drop_fn_by_adt,
                    )?;
                }
            }
            Callee::Intrinsic(IntrinsicOp::ScopedCallback) => {
                for (slot_idx, want_role) in [
                    (0usize, ForeignCallbackRole::ScopedRegister),
                    (1usize, ForeignCallbackRole::ScopedClose),
                ] {
                    let Some(arg) = args.get(slot_idx) else {
                        return Err(fail(
                            "c_with_callback missing register/close provider argument",
                        ));
                    };
                    let Some(for_id) = state.exact_foreign(arg) else {
                        return Err(fail(
                            "c_with_callback requires path-unique statically known extern callback provider",
                        ));
                    };
                    let Some(fdecl) = module.foreigns.get(for_id.index()) else {
                        return Err(fail("c_with_callback references unknown ForeignId"));
                    };
                    if fdecl.callback_role != Some(want_role) {
                        return Err(fail(
                            "c_with_callback register/close provider role mismatch",
                        ));
                    }
                }
            }
            Callee::Intrinsic(IntrinsicOp::ThreadScope) => {
                if let Some(arg) = args.get(1)
                    && let Operand::Copy(p) | Operand::Move(p) = arg
                {
                    let is_task = zil_ir::body::type_place(p, &func.body.cx(&module.tys))
                        .ok()
                        .and_then(|t| match module.tys.get(t.ty()) {
                            Some(TyKind::Managed(inner)) => Some(*inner),
                            _ => None,
                        })
                        .is_some_and(|inner| {
                            matches!(
                                module.tys.get(inner),
                                Some(TyKind::Opaque(OpaqueTy::Task { .. }))
                            )
                        });
                    if is_task && !state.is_proved_transfer_task(arg) {
                        return Err(fail(
                            "thread_scope task transfer requires a Task proved on all paths to originate from a resume(transfer) worker",
                        ));
                    }
                }
            }
            Callee::Direct(_)
            | Callee::Indirect(_)
            | Callee::Closure { .. }
            | Callee::DynSlot { .. }
            | Callee::Intrinsic(_) => {}
        }
    }
    Ok(())
}

fn collect_drop_fns_recursive(
    tys: &zil_ir::body::TyTable,
    ty: zil_ir::body::TyId,
    drop_fn_by_adt: &std::collections::HashMap<zil_ir::body::AdtId, FnId>,
    seen: &mut Vec<zil_ir::body::TyId>,
    out: &mut Vec<FnId>,
) {
    use zil_ir::body::OpaqueTy;
    if seen.contains(&ty) {
        return;
    }
    seen.push(ty);
    match tys.get(ty) {
        Some(TyKind::Adt { adt, args }) => {
            if let Some(&dfn) = drop_fn_by_adt.get(adt)
                && !out.contains(&dfn)
            {
                out.push(dfn);
            }
            if let Some(def) = tys.get_adt(*adt) {
                for v in &def.variants {
                    for f in &v.fields {
                        let fty = tys.subst_lookup(f.ty, *args).unwrap_or(f.ty);
                        collect_drop_fns_recursive(tys, fty, drop_fn_by_adt, seen, out);
                    }
                }
            }
        }
        Some(TyKind::Tuple(elems)) => {
            for e in elems {
                collect_drop_fns_recursive(tys, *e, drop_fn_by_adt, seen, out);
            }
        }
        Some(
            TyKind::Array { element: inner, .. } | TyKind::Managed(inner) | TyKind::Desc(inner),
        ) => collect_drop_fns_recursive(tys, *inner, drop_fn_by_adt, seen, out),
        Some(TyKind::ErrorUnion { ok, err }) => {
            collect_drop_fns_recursive(tys, *ok, drop_fn_by_adt, seen, out);
            collect_drop_fns_recursive(tys, *err, drop_fn_by_adt, seen, out);
        }
        Some(TyKind::Opaque(
            OpaqueTy::Mutex { payload } | OpaqueTy::OwnedStore { element: payload },
        )) => {
            collect_drop_fns_recursive(tys, *payload, drop_fn_by_adt, seen, out);
        }
        Some(
            TyKind::Unit
            | TyKind::Bool
            | TyKind::Never
            | TyKind::Int(_)
            | TyKind::Float(_)
            | TyKind::Slice(_)
            | TyKind::Ref { .. }
            | TyKind::FnPtr { .. }
            | TyKind::Closure { .. }
            | TyKind::Dyn(_)
            | TyKind::OwnedDyn { .. }
            | TyKind::Param(_)
            | TyKind::Opaque(_),
        )
        | None => {}
    }
}

fn collect_mutex_payload_drop_fns(
    tys: &zil_ir::body::TyTable,
    ty: zil_ir::body::TyId,
    drop_fn_by_adt: &std::collections::HashMap<zil_ir::body::AdtId, FnId>,
    seen: &mut Vec<zil_ir::body::TyId>,
    out: &mut Vec<FnId>,
) {
    use zil_ir::body::OpaqueTy;
    if seen.contains(&ty) {
        return;
    }
    seen.push(ty);
    match tys.get(ty) {
        Some(TyKind::Opaque(OpaqueTy::Mutex { payload })) => {
            let mut inner_seen = Vec::new();
            collect_drop_fns_recursive(tys, *payload, drop_fn_by_adt, &mut inner_seen, out);
        }
        Some(
            TyKind::Managed(inner)
            | TyKind::Desc(inner)
            | TyKind::Ref { pointee: inner, .. }
            | TyKind::Array { element: inner, .. },
        ) => collect_mutex_payload_drop_fns(tys, *inner, drop_fn_by_adt, seen, out),
        Some(TyKind::Tuple(elems)) => {
            for e in elems {
                collect_mutex_payload_drop_fns(tys, *e, drop_fn_by_adt, seen, out);
            }
        }
        Some(TyKind::Adt { adt, args }) => {
            if let Some(def) = tys.get_adt(*adt) {
                for v in &def.variants {
                    for f in &v.fields {
                        let fty = tys.subst_lookup(f.ty, *args).unwrap_or(f.ty);
                        collect_mutex_payload_drop_fns(tys, fty, drop_fn_by_adt, seen, out);
                    }
                }
            }
        }
        Some(
            TyKind::Unit
            | TyKind::Bool
            | TyKind::Never
            | TyKind::Int(_)
            | TyKind::Float(_)
            | TyKind::Slice(_)
            | TyKind::ErrorUnion { .. }
            | TyKind::FnPtr { .. }
            | TyKind::Closure { .. }
            | TyKind::Dyn(_)
            | TyKind::OwnedDyn { .. }
            | TyKind::Param(_)
            | TyKind::Opaque(_),
        )
        | None => {}
    }
}

fn fn_transitively_calls_foreign(
    module: &Module,
    entry: FnId,
    closure_to_fn: &std::collections::HashMap<zil_ir::body::ClosureId, FnId>,
) -> bool {
    use zil_ir::body::{Callee, Terminator};
    let mut visited = std::collections::HashSet::new();
    let mut worklist = vec![entry];
    while let Some(cur) = worklist.pop() {
        if !visited.insert(cur) {
            continue;
        }
        let Some(func) = module.functions.get(cur.index()) else {
            continue;
        };
        for block in &func.body.blocks {
            if let Terminator::Call { callee, .. } = &block.terminator {
                match callee {
                    Callee::Foreign(_) => return true,
                    Callee::Direct(target) => worklist.push(*target),
                    Callee::Closure { closure, .. } => {
                        if let Ok(cty) =
                            zil_ir::body::type_place(closure, &func.body.cx(&module.tys))
                            && let Some(TyKind::Closure { def, .. }) = module.tys.get(cty.ty())
                            && let Some(&cfn) = closure_to_fn.get(def)
                        {
                            worklist.push(cfn);
                        }
                    }
                    Callee::Indirect(_) | Callee::DynSlot { .. } | Callee::Intrinsic(_) => {}
                }
            }
        }
    }
    false
}
