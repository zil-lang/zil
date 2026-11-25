//! Transitive `Suspend` effect summaries and execution-context boundaries.

use zil_ir::body::{AdtId, FnId, FnRole, Module, TyKind};

use crate::provenance::{
    collect_mutex_payload_drop_fns, fn_transitively_calls_foreign, ty_contains_mutex_guard,
    ty_contains_raw_foreign_handle,
};
use crate::{CallScc, CheckError, CheckErrorKind};

/// Computes transitive `Suspend` (`TaskYield`) effect summaries over the call-graph
/// SCCs and enforces execution-context and synchronous-contract boundaries:
/// - Suspending functions cannot escape via `Rvalue::FnRef` except directly into
///   `TaskCreate` / `TaskStart` / `AsyncRegistrationStart` worker entry slots;
/// - `ThreadScope` callbacks cannot suspend (bare OS thread has no Task frame);
/// - `FnRole::DynSlotImpl`, `FnRole::Drop`, and entry `main` cannot transitively suspend.
pub(crate) fn check_suspend_effects(
    module: &Module,
    sccs: &[CallScc],
) -> Result<Vec<bool>, CheckError> {
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
