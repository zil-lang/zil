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
    check_contracts(&module, &can_suspend)?;
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
fn check_contracts(module: &Module, can_suspend: &[bool]) -> Result<(), CheckError> {
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
        //    `provides(retired(...))`, `provides(on_pending/on_ready(...))`).
        if !func.sig.state_contracts.is_empty() {
            verify_fn_state_contracts(module, fn_id, func)?;
        }
    }
    Ok(())
}

/// Independently verifies a function's `StateContractClause`s via forward CFG
/// abstract interpretation over `{Open, Closing, Closed}` + `retired`.
fn verify_fn_state_contracts(
    module: &Module,
    fn_id: FnId,
    func: &zil_ir::body::Function,
) -> Result<(), CheckError> {
    use std::collections::{HashMap, VecDeque};
    use zil_ir::body::{
        AggregateKind, BlockId, Callee, InstructionKind, IntrinsicOp, Local, OpaqueTy, Operand,
        PlaceRoot, ProtocolStateSet, Rvalue, StateContractClause, Terminator,
    };

    let tys = &module.tys;
    let fail = |reason: &'static str| CheckError {
        function: fn_id,
        kind: CheckErrorKind::CertificateShape(reason),
    };

    let is_protocol_handle_ty = |mut ty: zil_ir::body::TyId| -> bool {
        for _ in 0..4 {
            match tys.get(ty) {
                Some(
                    TyKind::Ref { pointee, .. } | TyKind::Managed(pointee) | TyKind::Desc(pointee),
                ) => ty = *pointee,
                Some(TyKind::Opaque(
                    OpaqueTy::Task { .. }
                    | OpaqueTy::Registration { .. }
                    | OpaqueTy::AsyncRegistration { .. },
                )) => return true,
                _ => return false,
            }
        }
        false
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

    let param_count = func.body.param_count as usize;
    let mut init_states = vec![ProtocolStateSet::ALL; param_count];
    let mut has_poll_post = false;
    for clause in &func.sig.state_contracts {
        let (param, states_opt) = match *clause {
            StateContractClause::RequiresState { param, states } => (param, Some(states)),
            StateContractClause::ProvidesRetired { param } => (param, None),
            StateContractClause::ProvidesOnPending { param, states }
            | StateContractClause::ProvidesOnReady { param, states } => {
                has_poll_post = true;
                (param, Some(states))
            }
        };
        let p_idx = param as usize;
        if p_idx >= param_count || p_idx >= func.sig.params.len() {
            return Err(fail("state contract references out-of-range parameter"));
        }
        if !is_protocol_handle_ty(func.sig.params[p_idx]) {
            return Err(fail(
                "state contract parameter is not a protocol handle type",
            ));
        }
        if let Some(st) = states_opt {
            if !st.is_valid() {
                return Err(fail("state contract has invalid ProtocolStateSet bitmask"));
            }
            if matches!(clause, StateContractClause::RequiresState { .. }) {
                init_states[p_idx] = st;
            }
        }
    }
    if has_poll_post && !is_poll_ty(func.sig.result) {
        return Err(fail(
            "on_pending/on_ready state contract requires Poll return type",
        ));
    }

    #[derive(Clone, PartialEq, Eq)]
    struct ProtoState {
        states: Vec<ProtocolStateSet>,
        retired: Vec<bool>,
        /// Maps a local to the protocol parameter index it aliases.
        alias: HashMap<Local, usize>,
        /// Maps a Poll local to `(param_idx, pending_states, ready_states)`.
        poll_of: HashMap<Local, (usize, ProtocolStateSet, ProtocolStateSet)>,
        /// Maps a discriminant local to the Poll local it was read from.
        discr_of: HashMap<Local, Local>,
        /// Maps a Poll local constructed directly from an enum variant (`0 = Pending, 1 = Ready`).
        poll_variant: HashMap<Local, u32>,
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
                let ret = self.retired[i] && other.retired[i];
                if ret != self.retired[i] {
                    self.retired[i] = ret;
                    changed = true;
                }
            }
            changed
        }

        fn resolve_param(&self, op: &Operand) -> Option<usize> {
            match op {
                Operand::Copy(p) | Operand::Move(p) => match p.root {
                    PlaceRoot::Local(l) => self.alias.get(&l).copied(),
                    PlaceRoot::Capture(_) => None,
                },
                Operand::Const(_) | Operand::Pooled(_) => None,
            }
        }
    }

    let mut entry_alias = HashMap::new();
    for i in 0..param_count {
        entry_alias.insert(Local(i as u32), i);
    }
    let entry_state = ProtoState {
        states: init_states,
        retired: vec![false; param_count],
        alias: entry_alias,
        poll_of: HashMap::new(),
        discr_of: HashMap::new(),
        poll_variant: HashMap::new(),
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

    while let Some(bi) = queue.pop_front() {
        let mut st = in_states[bi].clone().expect("enqueued block has state");
        let block = &func.body.blocks[bi];

        for inst in &block.instructions {
            if let InstructionKind::Assign { dest, value } = &inst.kind
                && dest.projections.is_empty()
                && let PlaceRoot::Local(dl) = dest.root
            {
                st.alias.remove(&dl);
                st.poll_of.remove(&dl);
                st.discr_of.remove(&dl);
                st.poll_variant.remove(&dl);
                match value {
                    Rvalue::Use(Operand::Copy(src) | Operand::Move(src))
                    | Rvalue::Ref { place: src, .. }
                    | Rvalue::Describe { place: src } => {
                        if let PlaceRoot::Local(sl) = src.root {
                            if let Some(&p) = st.alias.get(&sl) {
                                st.alias.insert(dl, p);
                            }
                            if let Some(&po) = st.poll_of.get(&sl) {
                                st.poll_of.insert(dl, po);
                            }
                            if let Some(&pv) = st.poll_variant.get(&sl) {
                                st.poll_variant.insert(dl, pv);
                            }
                        }
                    }
                    Rvalue::Discriminant(src) => {
                        if src.projections.is_empty()
                            && let PlaceRoot::Local(sl) = src.root
                        {
                            st.discr_of.insert(dl, sl);
                        }
                    }
                    Rvalue::Aggregate {
                        kind: AggregateKind::Adt { variant, .. },
                        ..
                    } => {
                        st.poll_variant.insert(dl, variant.0);
                    }
                    Rvalue::Use(_)
                    | Rvalue::SliceRef { .. }
                    | Rvalue::Aggregate { .. }
                    | Rvalue::ErrorUnion { .. }
                    | Rvalue::Unary { .. }
                    | Rvalue::Binary { .. }
                    | Rvalue::Convert { .. }
                    | Rvalue::ErrorArm(_)
                    | Rvalue::Len(_)
                    | Rvalue::FnRef(_)
                    | Rvalue::ForeignFnRef(_)
                    | Rvalue::StaticUtf(_) => {}
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
                let poll_info = match discr {
                    Operand::Copy(p) | Operand::Move(p) if p.projections.is_empty() => {
                        match p.root {
                            PlaceRoot::Local(l) => st
                                .discr_of
                                .get(&l)
                                .and_then(|pl| st.poll_of.get(pl).copied()),
                            PlaceRoot::Capture(_) => None,
                        }
                    }
                    Operand::Copy(_)
                    | Operand::Move(_)
                    | Operand::Const(_)
                    | Operand::Pooled(_) => None,
                };
                for &(val, target) in arms {
                    let mut branch_st = st.clone();
                    if let Some((p_idx, pend_st, ready_st)) = poll_info {
                        if val == 0 {
                            branch_st.states[p_idx] = pend_st;
                        } else if val == 1 {
                            branch_st.states[p_idx] = ready_st;
                        }
                    }
                    enqueue(target, branch_st, &mut in_states, &mut queue);
                }
                enqueue(*otherwise, st, &mut in_states, &mut queue);
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
                let dest_local = if dest.projections.is_empty() {
                    match dest.root {
                        PlaceRoot::Local(l) => Some(l),
                        PlaceRoot::Capture(_) => None,
                    }
                } else {
                    None
                };
                match callee {
                    Callee::Intrinsic(
                        IntrinsicOp::TaskRequestCancel | IntrinsicOp::AsyncRegistrationRequestClose,
                    ) => {
                        if let Some(a0) = args.first()
                            && let Some(p_idx) = st.resolve_param(a0)
                        {
                            st.states[p_idx] = ProtocolStateSet(
                                ProtocolStateSet::CLOSING | ProtocolStateSet::CLOSED,
                            );
                        }
                    }
                    Callee::Intrinsic(
                        IntrinsicOp::TaskPollClose | IntrinsicOp::AsyncRegistrationPollClose,
                    ) => {
                        if let Some(a0) = args.first()
                            && let Some(p_idx) = st.resolve_param(a0)
                        {
                            st.states[p_idx] = ProtocolStateSet(
                                ProtocolStateSet::CLOSING | ProtocolStateSet::CLOSED,
                            );
                            if let Some(dl) = dest_local {
                                st.poll_of.insert(
                                    dl,
                                    (
                                        p_idx,
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
                            && let Some(p_idx) = st.resolve_param(a0)
                        {
                            if !st.states[p_idx]
                                .is_subset_of(ProtocolStateSet(ProtocolStateSet::CLOSED))
                            {
                                return Err(fail(
                                    "finish_destroy called on protocol handle not proved Closed",
                                ));
                            }
                            st.retired[p_idx] = true;
                        }
                    }
                    Callee::Intrinsic(IntrinsicOp::RegistrationClose) => {
                        if let Some(a0) = args.first()
                            && let Some(p_idx) = st.resolve_param(a0)
                        {
                            st.retired[p_idx] = true;
                        }
                    }
                    Callee::Direct(cid) => {
                        if let Some(callee_fn) = module.functions.get(cid.index())
                            && !callee_fn.sig.state_contracts.is_empty()
                        {
                            let mut pend_by_param: HashMap<usize, ProtocolStateSet> =
                                HashMap::new();
                            let mut ready_by_param: HashMap<usize, ProtocolStateSet> =
                                HashMap::new();
                            for c in &callee_fn.sig.state_contracts {
                                match *c {
                                    StateContractClause::RequiresState { param, states } => {
                                        if let Some(arg) = args.get(param as usize)
                                            && let Some(p_idx) = st.resolve_param(arg)
                                            && !st.states[p_idx].is_subset_of(states)
                                        {
                                            return Err(fail(
                                                "callee requires(state(...)) precondition not satisfied",
                                            ));
                                        }
                                    }
                                    StateContractClause::ProvidesRetired { param } => {
                                        if let Some(arg) = args.get(param as usize)
                                            && let Some(p_idx) = st.resolve_param(arg)
                                        {
                                            st.retired[p_idx] = true;
                                        }
                                    }
                                    StateContractClause::ProvidesOnPending { param, states } => {
                                        if let Some(arg) = args.get(param as usize)
                                            && let Some(p_idx) = st.resolve_param(arg)
                                        {
                                            pend_by_param.insert(p_idx, states);
                                        }
                                    }
                                    StateContractClause::ProvidesOnReady { param, states } => {
                                        if let Some(arg) = args.get(param as usize)
                                            && let Some(p_idx) = st.resolve_param(arg)
                                        {
                                            ready_by_param.insert(p_idx, states);
                                        }
                                    }
                                }
                            }
                            if let Some(dl) = dest_local {
                                for (&p_idx, &pend) in &pend_by_param {
                                    let ready = ready_by_param
                                        .get(&p_idx)
                                        .copied()
                                        .unwrap_or(st.states[p_idx]);
                                    st.poll_of.insert(dl, (p_idx, pend, ready));
                                }
                                for (&p_idx, &ready) in &ready_by_param {
                                    if !pend_by_param.contains_key(&p_idx) {
                                        st.poll_of.insert(dl, (p_idx, st.states[p_idx], ready));
                                    }
                                }
                            }
                        }
                    }
                    Callee::Intrinsic(_)
                    | Callee::Indirect(_)
                    | Callee::Closure { .. }
                    | Callee::DynSlot { .. }
                    | Callee::Foreign(_) => {}
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
                        StateContractClause::RequiresState { .. } => {}
                        StateContractClause::ProvidesRetired { param } => {
                            if !st.retired[param as usize] {
                                return Err(fail(
                                    "function fails to satisfy provides(retired(...)) on all return paths",
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
                                && st.poll_variant.get(&rl) == Some(&1)
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
                                && st.poll_variant.get(&rl) == Some(&0)
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
