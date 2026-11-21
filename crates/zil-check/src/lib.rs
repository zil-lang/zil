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
    check_suspend_effects(&module, &sccs)?;
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
            if func.sig.type_params != 0 || func.sig.region_params != 0 {
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
                        if *mutability == want_mut && *pointee == dimpl.concrete_ty
                ),
                None => recv_ty == dimpl.concrete_ty,
            };
            if !recv_ok {
                return Err(fail(fn_id, "dyn impl method receiver type mismatch"));
            }
            if func.sig.params[1..] != slot.params[..] {
                return Err(fail(fn_id, "dyn impl method parameter type mismatch"));
            }
            if func.sig.result != slot.result {
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

/// Computes transitive `Suspend` (`TaskYield`) effect summaries over the call-graph
/// SCCs and enforces execution-context and synchronous-contract boundaries:
/// - Suspending functions cannot escape via `Rvalue::FnRef` except directly into
///   `TaskCreate` / `TaskStart` / `AsyncRegistrationStart` worker entry slots;
/// - `ThreadScope` callbacks cannot suspend (bare OS thread has no Task frame);
/// - `FnRole::DynSlotImpl`, `FnRole::Drop`, and entry `main` cannot transitively suspend.
fn check_suspend_effects(module: &Module, sccs: &[CallScc]) -> Result<(), CheckError> {
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
                    if *op == IntrinsicOp::AsyncRegistrationStart {
                        let Some(entry_fn) = resolved_cfn else {
                            return Err(fail(
                                fid,
                                "async_registration.start callback must resolve to a statically proved sealed ThreadCallable closure",
                            ));
                        };
                        verify_sealed_thread_callable(
                            module,
                            fid,
                            entry_fn,
                            &closure_to_fn,
                            &drop_fn_by_adt,
                        )?;
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

    Ok(())
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
                | TyKind::OwnedDyn(_)
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
                        | IntrinsicOp::SyncCompareExchange => {}
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
                        | IntrinsicOp::ThreadScope => {
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
