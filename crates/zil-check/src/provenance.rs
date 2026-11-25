//! Forward CFG provenance verification for `AsyncRegistrationStart`, `ScopedCallback`,
//! `ThreadScope` task transfer, sealed `ThreadCallable` profiles, and FFI drop boundaries.

use zil_ir::body::{FnId, FnRole, Module, TyKind};

use crate::{CheckError, CheckErrorKind};

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

pub(crate) fn ty_contains_mutex_guard(
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

pub(crate) fn ty_contains_raw_foreign_handle(
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

pub(crate) fn verify_fn_provenance(
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
                        && matches!(
                            callee,
                            Callee::Intrinsic(IntrinsicOp::TaskCreate | IntrinsicOp::TaskStart)
                        )
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

pub(crate) fn collect_mutex_payload_drop_fns(
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

pub(crate) fn fn_transitively_calls_foreign(
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
