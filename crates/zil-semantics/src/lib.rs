//! The single semantic rule kernel (`spec/semantic-kernel.md` §5).
//!
//! Holds the state domain, its order and join, the transfer of every
//! operation and edge, and exit obligations. It contains no fixpoint
//! scheduling, no diagnostic policy and cannot construct a verified module:
//! `zil-solve` iterates these rules, `zil-check` applies them once per
//! operation and edge to an untrusted certificate.
#![forbid(unsafe_code)]
#![deny(clippy::wildcard_enum_match_arm)]

mod borrow;
mod loans;
mod paths;
pub mod protocol;
mod state;
mod transfer;

pub use loans::{EXTERNAL, Liveness, Loan, LoanKind, LoanTable};
pub use paths::{MovePaths, PathId, tracked_len};
pub use protocol::{
    HandleProtocolSlot, ProtoHandleInfo, StoreProtocolSummary, TASK_OR_ASYNC_REG_POLL_CLOSE_STATES,
    proto_handle_info, store_element_proto_info, task_or_async_reg_request_close_next,
    task_poll_next,
};
pub use state::{BitSet, Carries, InitState, MustFacts, State};
pub use transfer::{Kernel, Successor, Violation, ViolationKind};

/// Reference to the base crate keeps spans available to violation
/// rendering without a second schema.
pub use zil_base::Span;

/// Computes a sound, deterministic per-function return flow summary:
/// which parameter indices (`0..64`), closure capture indices (`0..64`),
/// and (when returning a closure) per-field parameter origins may flow into
/// each function's return value.
///
/// Both `zil-solve` and `zil-check` derive this independently from the IR
/// module (never from untrusted certificate claims), so a forged certificate
/// cannot weaken caller borrow checking.
pub(crate) fn is_structural_mutation(op: zil_ir::body::IntrinsicOp) -> bool {
    use zil_ir::body::IntrinsicOp as I;
    matches!(
        op,
        I::StoreReserve
            | I::StorePush
            | I::StorePop
            | I::ByteBufferReserve
            | I::ByteBufferPush
            | I::ByteBufferTruncate
    )
}

pub(crate) fn ty_holds_reference(tys: &zil_ir::body::TyTable, ty: zil_ir::body::TyId) -> bool {
    fn walk(
        tys: &zil_ir::body::TyTable,
        ty: zil_ir::body::TyId,
        seen: &mut Vec<zil_ir::body::TyId>,
    ) -> bool {
        use zil_ir::body::TyKind;
        if seen.contains(&ty) {
            return false;
        }
        seen.push(ty);
        let Some(kind) = tys.get(ty) else {
            return false;
        };
        match kind {
            TyKind::Ref { .. } => true,
            TyKind::Desc(_) => false,
            TyKind::Unit
            | TyKind::Never
            | TyKind::Bool
            | TyKind::Int(_)
            | TyKind::Float(_)
            | TyKind::Param(_)
            | TyKind::FnPtr { .. }
            | TyKind::Opaque(_) => false,
            TyKind::Managed(inner)
            | TyKind::Slice(inner)
            | TyKind::Array { element: inner, .. } => walk(tys, *inner, seen),
            TyKind::Tuple(elems) => elems.iter().any(|e| walk(tys, *e, seen)),
            TyKind::ErrorUnion { ok, err } => walk(tys, *ok, seen) || walk(tys, *err, seen),
            TyKind::Closure { def, args } => tys.closure(*def).is_some_and(|c| {
                c.captures.iter().any(|cap| {
                    tys.subst_lookup(*cap, *args)
                        .is_some_and(|t| walk(tys, t, seen))
                })
            }),
            TyKind::Adt { adt, args } => tys.get_adt(*adt).is_some_and(|def| {
                def.variants.iter().any(|v| {
                    v.fields.iter().any(|f| {
                        tys.subst_lookup(f.ty, *args)
                            .is_some_and(|t| walk(tys, t, seen))
                    })
                })
            }),
            TyKind::Dyn { .. } => false,
            TyKind::OwnedDyn { context, .. } => context.is_some_and(|c| walk(tys, c, seen)),
        }
    }
    walk(tys, ty, &mut Vec::new())
}

pub(crate) fn ty_holds_store_or_bytes_desc(
    tys: &zil_ir::body::TyTable,
    ty: zil_ir::body::TyId,
) -> bool {
    fn walk(
        tys: &zil_ir::body::TyTable,
        ty: zil_ir::body::TyId,
        seen: &mut Vec<zil_ir::body::TyId>,
    ) -> bool {
        use zil_ir::body::{OpaqueTy, TyKind};
        if seen.contains(&ty) {
            return false;
        }
        seen.push(ty);
        let Some(kind) = tys.get(ty) else {
            return false;
        };
        match kind {
            TyKind::Desc(inner) => matches!(
                tys.get(*inner),
                Some(TyKind::Opaque(
                    OpaqueTy::OwnedStore { .. } | OpaqueTy::ByteBuffer
                ))
            ),
            TyKind::Unit
            | TyKind::Never
            | TyKind::Bool
            | TyKind::Int(_)
            | TyKind::Float(_)
            | TyKind::Param(_)
            | TyKind::FnPtr { .. }
            | TyKind::Opaque(_)
            | TyKind::Dyn { .. } => false,
            TyKind::OwnedDyn { context, .. } => context.is_some_and(|c| walk(tys, c, seen)),
            TyKind::Ref { pointee: inner, .. }
            | TyKind::Managed(inner)
            | TyKind::Slice(inner)
            | TyKind::Array { element: inner, .. } => walk(tys, *inner, seen),
            TyKind::Tuple(elems) => elems.iter().any(|e| walk(tys, *e, seen)),
            TyKind::ErrorUnion { ok, err } => walk(tys, *ok, seen) || walk(tys, *err, seen),
            TyKind::Closure { def, args } => tys.closure(*def).is_some_and(|c| {
                c.captures.iter().any(|cap| {
                    tys.subst_lookup(*cap, *args)
                        .is_some_and(|t| walk(tys, t, seen))
                })
            }),
            TyKind::Adt { adt, args } => tys.get_adt(*adt).is_some_and(|def| {
                def.variants.iter().any(|v| {
                    v.fields.iter().any(|f| {
                        tys.subst_lookup(f.ty, *args)
                            .is_some_and(|t| walk(tys, t, seen))
                    })
                })
            }),
        }
    }
    walk(tys, ty, &mut Vec::new())
}

pub fn compute_return_summaries(module: &zil_ir::body::Module) -> Vec<zil_ir::body::ReturnSummary> {
    use zil_ir::body::{
        AggregateKind, Callee, InstructionKind, IntrinsicOp, Operand, OperandRole, PlaceAccess,
        PlaceRoot, Projection, ReturnSummary, Rvalue, Terminator,
    };

    #[derive(Clone, PartialEq, Eq)]
    enum FieldFlow {
        Unset,
        Known(Vec<u64>),
        Top,
    }

    impl FieldFlow {
        fn join_with(&mut self, other: &FieldFlow) -> bool {
            match (&mut *self, other) {
                (FieldFlow::Top, _) | (_, FieldFlow::Unset) => false,
                (FieldFlow::Unset, FieldFlow::Known(v)) => {
                    *self = FieldFlow::Known(v.clone());
                    true
                }
                (FieldFlow::Unset, FieldFlow::Top) => {
                    *self = FieldFlow::Top;
                    true
                }
                (FieldFlow::Known(a), FieldFlow::Known(b)) if a.len() == b.len() => {
                    let mut ch = false;
                    for (x, y) in a.iter_mut().zip(b) {
                        let next = *x | *y;
                        if next != *x {
                            *x = next;
                            ch = true;
                        }
                    }
                    ch
                }
                (FieldFlow::Known(_), FieldFlow::Known(_) | FieldFlow::Top) => {
                    *self = FieldFlow::Top;
                    true
                }
            }
        }
    }

    let op_place_deps = |op: &Operand, deps: &[u64]| -> u64 {
        match op {
            Operand::Copy(p) | Operand::Move(p) => match p.root {
                PlaceRoot::Local(l) => deps.get(l.index()).copied().unwrap_or(0),
                PlaceRoot::Capture(_) => 0,
            },
            Operand::Const(_) | Operand::Pooled(_) => 0,
        }
    };

    let op_place_cap_deps = |op: &Operand, cap_deps: &[u64]| -> u64 {
        match op {
            Operand::Copy(p) | Operand::Move(p) => match p.root {
                PlaceRoot::Local(l) => cap_deps.get(l.index()).copied().unwrap_or(0),
                PlaceRoot::Capture(ci) => {
                    if ci < 64 {
                        1u64 << ci
                    } else {
                        u64::MAX
                    }
                }
            },
            Operand::Const(_) | Operand::Pooled(_) => 0,
        }
    };

    let n = module.functions.len();
    let mut summaries: Vec<ReturnSummary> = module
        .functions
        .iter()
        .map(|f| ReturnSummary {
            closure: f.body.closure,
            ..ReturnSummary::default()
        })
        .collect();
    let mut field_flows = vec![FieldFlow::Unset; n];

    let mut closure_to_fn: std::collections::HashMap<zil_ir::body::ClosureId, usize> =
        std::collections::HashMap::new();
    for (fi, func) in module.functions.iter().enumerate() {
        if let Some(cid) = func.body.closure {
            closure_to_fn.insert(cid, fi);
        }
    }
    let mut dependents: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut closure_callers: Vec<usize> = Vec::new();
    for (fi, func) in module.functions.iter().enumerate() {
        let mut has_closure_consumer = false;
        for block in &func.body.blocks {
            for inst in &block.instructions {
                if let InstructionKind::Assign { value, .. } = &inst.kind {
                    match value {
                        Rvalue::FnRef(cid) if cid.index() < n => {
                            dependents[cid.index()].push(fi);
                        }
                        Rvalue::Aggregate {
                            kind: AggregateKind::Closure { def, .. },
                            ..
                        } => {
                            if let Some(&cfn) = closure_to_fn.get(def) {
                                dependents[cfn].push(fi);
                            }
                        }
                        Rvalue::Use(_)
                        | Rvalue::Ref { .. }
                        | Rvalue::SliceRef { .. }
                        | Rvalue::Describe { .. }
                        | Rvalue::Aggregate { .. }
                        | Rvalue::ErrorUnion { .. }
                        | Rvalue::Unary { .. }
                        | Rvalue::Binary { .. }
                        | Rvalue::Convert { .. }
                        | Rvalue::Discriminant(_)
                        | Rvalue::ErrorArm(_)
                        | Rvalue::Len(_)
                        | Rvalue::FnRef(_)
                        | Rvalue::ForeignFnRef(_)
                        | Rvalue::StaticUtf(_) => {}
                    }
                }
            }
            if let Terminator::Call { callee, .. } = &block.terminator {
                match callee {
                    Callee::Direct(cid) if cid.index() < n => {
                        dependents[cid.index()].push(fi);
                    }
                    Callee::Closure { .. }
                    | Callee::Intrinsic(
                        IntrinsicOp::ThreadScope
                        | IntrinsicOp::ScopedCallback
                        | IntrinsicOp::TaskCreate
                        | IntrinsicOp::TaskStart
                        | IntrinsicOp::AsyncRegistrationStart,
                    ) => has_closure_consumer = true,
                    Callee::Direct(_)
                    | Callee::Indirect(_)
                    | Callee::DynSlot { .. }
                    | Callee::Foreign(_)
                    | Callee::Intrinsic(_) => {}
                }
            }
        }
        if has_closure_consumer {
            closure_callers.push(fi);
        }
    }
    for deps in &mut dependents {
        deps.sort_unstable();
        deps.dedup();
    }
    let mut dirty = vec![true; n];
    let mut next_dirty = vec![false; n];
    let mut changed = true;
    let max_rounds = n.saturating_mul(64).max(1) + 1;
    for _ in 0..max_rounds {
        if !changed {
            break;
        }
        changed = false;
        next_dirty.fill(false);
        for (fi, func) in module.functions.iter().enumerate() {
            if !dirty[fi] {
                continue;
            }
            let body = &func.body;
            let param_count = body.params().count();
            if param_count >= 64 {
                if summaries[fi].param_mask != u64::MAX
                    || summaries[fi].capture_mask != u64::MAX
                    || summaries[fi].mutating_params != u64::MAX
                    || summaries[fi].mutating_captures != u64::MAX
                    || summaries[fi].addr_store_params != u64::MAX
                    || summaries[fi].addr_store_captures != u64::MAX
                    || field_flows[fi] != FieldFlow::Top
                {
                    summaries[fi].param_mask = u64::MAX;
                    summaries[fi].capture_mask = u64::MAX;
                    summaries[fi].mutating_params = u64::MAX;
                    summaries[fi].mutating_captures = u64::MAX;
                    summaries[fi].addr_store_params = u64::MAX;
                    summaries[fi].addr_store_captures = u64::MAX;
                    summaries[fi].ret_field_param_masks.clear();
                    field_flows[fi] = FieldFlow::Top;
                    changed = true;
                    for &dep in &dependents[fi] {
                        next_dirty[dep] = true;
                    }
                    if func.body.closure.is_some() {
                        for &cc in &closure_callers {
                            next_dirty[cc] = true;
                        }
                    }
                }
                continue;
            }
            let all_params_mask = if param_count == 0 {
                0u64
            } else {
                (1u64 << param_count) - 1
            };
            let mut deps = vec![0u64; body.locals.len()];
            let mut cap_deps = vec![0u64; body.locals.len()];
            let mut local_fields = vec![FieldFlow::Unset; body.locals.len()];
            let mut local_cap_fields = vec![FieldFlow::Unset; body.locals.len()];
            for (pi, p) in body.params().enumerate() {
                if p.index() < deps.len() {
                    deps[p.index()] = 1u64 << pi;
                    local_fields[p.index()] = FieldFlow::Top;
                    local_cap_fields[p.index()] = FieldFlow::Top;
                }
            }

            let mut local_changed = true;
            let mut ret_param = summaries[fi].param_mask;
            let mut ret_cap = summaries[fi].capture_mask;
            let mut mut_param = summaries[fi].mutating_params;
            let mut mut_cap = summaries[fi].mutating_captures;
            let mut addr_store_param = summaries[fi].addr_store_params;
            let mut addr_store_cap = summaries[fi].addr_store_captures;
            let mut ret_fields = field_flows[fi].clone();

            let cx = body.cx(&module.tys);
            let local_holds_addr: Vec<bool> = body
                .locals
                .iter()
                .map(|d| module.tys.holds_address(d.ty))
                .collect();
            let place_holds_addr = |p: &zil_ir::body::Place| -> bool {
                if p.projections.is_empty()
                    && let PlaceRoot::Local(l) = p.root
                    && let Some(&holds) = local_holds_addr.get(l.index())
                {
                    return holds;
                }
                zil_ir::body::type_place(p, &cx).map_or(true, |t| module.tys.holds_address(t.ty()))
            };

            while local_changed {
                local_changed = false;
                for block in &body.blocks {
                    for inst in &block.instructions {
                        if !matches!(
                            &inst.kind,
                            InstructionKind::Assign { .. } | InstructionKind::Replace { .. }
                        ) {
                            continue;
                        }
                        let accesses = inst.kind.accesses();
                        match &inst.kind {
                            InstructionKind::Assign { dest, value } => {
                                let dest_holds = place_holds_addr(dest);
                                let mut rhs_p = 0u64;
                                let mut rhs_c = 0u64;
                                if dest_holds {
                                    for a in &accesses {
                                        let carries_addr = matches!(
                                            a.access,
                                            PlaceAccess::Borrow(_) | PlaceAccess::Describe
                                        ) || place_holds_addr(a.place.as_ref());
                                        if a.access != PlaceAccess::Init && carries_addr {
                                            match a.place.root {
                                                PlaceRoot::Local(l) => {
                                                    if l.index() < deps.len() {
                                                        rhs_p |= deps[l.index()];
                                                        rhs_c |= cap_deps[l.index()];
                                                    }
                                                }
                                                PlaceRoot::Capture(ci) => {
                                                    if ci < 64 {
                                                        rhs_c |= 1u64 << ci;
                                                    } else {
                                                        rhs_c = u64::MAX;
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                if dest_holds
                                    && (dest.first_deref().is_some()
                                        || matches!(dest.root, PlaceRoot::Capture(_)))
                                {
                                    match dest.root {
                                        PlaceRoot::Local(l) if l.index() < deps.len() => {
                                            addr_store_param |= deps[l.index()];
                                            addr_store_cap |= cap_deps[l.index()];
                                        }
                                        PlaceRoot::Capture(ci) => {
                                            addr_store_cap |=
                                                if ci < 64 { 1u64 << ci } else { u64::MAX };
                                        }
                                        PlaceRoot::Local(_) => {}
                                    }
                                }
                                if dest
                                    .projections
                                    .iter()
                                    .any(|p| matches!(p, Projection::Deref))
                                {
                                    if rhs_p != 0 || rhs_c != 0 {
                                        for (d, c) in deps.iter_mut().zip(&mut cap_deps) {
                                            let np = *d | rhs_p;
                                            let nc = *c | rhs_c;
                                            if np != *d || nc != *c {
                                                *d = np;
                                                *c = nc;
                                                local_changed = true;
                                            }
                                        }
                                        for lf in &mut local_fields {
                                            if *lf != FieldFlow::Top {
                                                *lf = FieldFlow::Top;
                                                local_changed = true;
                                            }
                                        }
                                        for lcf in &mut local_cap_fields {
                                            if *lcf != FieldFlow::Top {
                                                *lcf = FieldFlow::Top;
                                                local_changed = true;
                                            }
                                        }
                                    }
                                } else if let PlaceRoot::Local(l) = dest.root
                                    && l.index() < deps.len()
                                {
                                    let np = deps[l.index()] | rhs_p;
                                    let nc = cap_deps[l.index()] | rhs_c;
                                    if np != deps[l.index()] || nc != cap_deps[l.index()] {
                                        deps[l.index()] = np;
                                        cap_deps[l.index()] = nc;
                                        local_changed = true;
                                    }
                                    let (rhs_flow, rhs_cap_flow) = if !dest.projections.is_empty() {
                                        (FieldFlow::Top, FieldFlow::Top)
                                    } else {
                                        match value {
                                            Rvalue::Aggregate { kind, operands } => {
                                                let track = match kind {
                                                    AggregateKind::Closure { .. }
                                                    | AggregateKind::Tuple => true,
                                                    AggregateKind::Adt { adt, .. } => module
                                                        .tys
                                                        .get_adt(*adt)
                                                        .is_some_and(|def| match def.kind {
                                                            zil_ir::body::AdtKind::Record
                                                            | zil_ir::body::AdtKind::Package
                                                            | zil_ir::body::AdtKind::Resource => {
                                                                true
                                                            }
                                                            zil_ir::body::AdtKind::Enum => false,
                                                        }),
                                                    AggregateKind::Array { .. } => false,
                                                };
                                                if track {
                                                    (
                                                        FieldFlow::Known(
                                                            operands
                                                                .iter()
                                                                .map(|f| op_place_deps(f, &deps))
                                                                .collect(),
                                                        ),
                                                        FieldFlow::Known(
                                                            operands
                                                                .iter()
                                                                .map(|f| {
                                                                    op_place_cap_deps(f, &cap_deps)
                                                                })
                                                                .collect(),
                                                        ),
                                                    )
                                                } else {
                                                    (FieldFlow::Top, FieldFlow::Top)
                                                }
                                            }
                                            Rvalue::Use(Operand::Copy(p) | Operand::Move(p))
                                                if p.projections.is_empty() =>
                                            {
                                                match p.root {
                                                    PlaceRoot::Local(src)
                                                        if src.index() < local_fields.len() =>
                                                    {
                                                        (
                                                            local_fields[src.index()].clone(),
                                                            local_cap_fields[src.index()].clone(),
                                                        )
                                                    }
                                                    PlaceRoot::Local(_) | PlaceRoot::Capture(_) => {
                                                        (FieldFlow::Top, FieldFlow::Top)
                                                    }
                                                }
                                            }
                                            Rvalue::Use(_)
                                            | Rvalue::ErrorUnion { .. }
                                            | Rvalue::Ref { .. }
                                            | Rvalue::SliceRef { .. }
                                            | Rvalue::Describe { .. }
                                            | Rvalue::Unary { .. }
                                            | Rvalue::Binary { .. }
                                            | Rvalue::Convert { .. }
                                            | Rvalue::Discriminant(_)
                                            | Rvalue::ErrorArm(_)
                                            | Rvalue::Len(_)
                                            | Rvalue::FnRef(_)
                                            | Rvalue::ForeignFnRef(_)
                                            | Rvalue::StaticUtf(_) => {
                                                (FieldFlow::Top, FieldFlow::Top)
                                            }
                                        }
                                    };
                                    if local_fields[l.index()].join_with(&rhs_flow) {
                                        local_changed = true;
                                    }
                                    if local_cap_fields[l.index()].join_with(&rhs_cap_flow) {
                                        local_changed = true;
                                    }
                                }
                            }
                            InstructionKind::Replace { place, old, .. } => {
                                if place_holds_addr(place)
                                    && (place.first_deref().is_some()
                                        || matches!(place.root, PlaceRoot::Capture(_)))
                                {
                                    match place.root {
                                        PlaceRoot::Local(l) if l.index() < deps.len() => {
                                            addr_store_param |= deps[l.index()];
                                            addr_store_cap |= cap_deps[l.index()];
                                        }
                                        PlaceRoot::Capture(ci) => {
                                            addr_store_cap |=
                                                if ci < 64 { 1u64 << ci } else { u64::MAX };
                                        }
                                        PlaceRoot::Local(_) => {}
                                    }
                                }
                                let mut val_p = 0u64;
                                let mut val_c = 0u64;
                                let mut old_p = 0u64;
                                let mut old_c = 0u64;
                                for a in &accesses {
                                    let (p_bits, c_bits) = match a.place.root {
                                        PlaceRoot::Local(l) if l.index() < deps.len() => {
                                            (deps[l.index()], cap_deps[l.index()])
                                        }
                                        PlaceRoot::Capture(ci) => {
                                            (0u64, if ci < 64 { 1u64 << ci } else { u64::MAX })
                                        }
                                        PlaceRoot::Local(_) => (0u64, 0u64),
                                    };
                                    match a.role {
                                        zil_ir::body::OperandRole::Value => {
                                            val_p |= p_bits;
                                            val_c |= c_bits;
                                        }
                                        zil_ir::body::OperandRole::Target => {
                                            old_p |= p_bits;
                                            old_c |= c_bits;
                                        }
                                        zil_ir::body::OperandRole::ReplacedOld
                                        | zil_ir::body::OperandRole::Index
                                        | zil_ir::body::OperandRole::Component(_)
                                        | zil_ir::body::OperandRole::Lhs
                                        | zil_ir::body::OperandRole::Rhs
                                        | zil_ir::body::OperandRole::Arg(_)
                                        | zil_ir::body::OperandRole::Callee
                                        | zil_ir::body::OperandRole::Discriminant
                                        | zil_ir::body::OperandRole::Condition
                                        | zil_ir::body::OperandRole::ReturnValue
                                        | zil_ir::body::OperandRole::DeferClosure => {}
                                    }
                                }
                                if let PlaceRoot::Local(l) = place.root
                                    && l.index() < deps.len()
                                {
                                    let np = deps[l.index()] | val_p;
                                    let nc = cap_deps[l.index()] | val_c;
                                    if np != deps[l.index()] || nc != cap_deps[l.index()] {
                                        deps[l.index()] = np;
                                        cap_deps[l.index()] = nc;
                                        local_changed = true;
                                    }
                                    if local_fields[l.index()].join_with(&FieldFlow::Top) {
                                        local_changed = true;
                                    }
                                }
                                if old.index() < deps.len() {
                                    let np = deps[old.index()] | old_p;
                                    let nc = cap_deps[old.index()] | old_c;
                                    if np != deps[old.index()] || nc != cap_deps[old.index()] {
                                        deps[old.index()] = np;
                                        cap_deps[old.index()] = nc;
                                        local_changed = true;
                                    }
                                    if local_fields[old.index()].join_with(&FieldFlow::Top) {
                                        local_changed = true;
                                    }
                                }
                            }
                            InstructionKind::Drop { .. }
                            | InstructionKind::StorageLive(_)
                            | InstructionKind::StorageDead(_)
                            | InstructionKind::ScopeEnter(_)
                            | InstructionKind::ScopeExit { .. }
                            | InstructionKind::RegisterDefer { .. } => {}
                        }
                    }

                    let accesses = block.terminator.accesses();
                    match &block.terminator {
                        Terminator::Return { value } => {
                            for a in &accesses {
                                match a.place.root {
                                    PlaceRoot::Local(l) if l.index() < deps.len() => {
                                        ret_param |= deps[l.index()];
                                        ret_cap |= cap_deps[l.index()];
                                    }
                                    PlaceRoot::Capture(ci) => {
                                        ret_cap |= if ci < 64 { 1u64 << ci } else { u64::MAX };
                                    }
                                    PlaceRoot::Local(_) => {}
                                }
                            }
                            let ret_f = match value {
                                Operand::Copy(p) | Operand::Move(p) if p.projections.is_empty() => {
                                    match p.root {
                                        PlaceRoot::Local(l) if l.index() < local_fields.len() => {
                                            local_fields[l.index()].clone()
                                        }
                                        PlaceRoot::Local(_) | PlaceRoot::Capture(_) => {
                                            FieldFlow::Top
                                        }
                                    }
                                }
                                Operand::Copy(_)
                                | Operand::Move(_)
                                | Operand::Const(_)
                                | Operand::Pooled(_) => FieldFlow::Top,
                            };
                            ret_fields.join_with(&ret_f);
                        }
                        Terminator::Call {
                            callee, args, dest, ..
                        } => {
                            let mut call_p = 0u64;
                            let mut call_c = 0u64;
                            let mut call_fields = FieldFlow::Top;
                            match callee {
                                Callee::Direct(g) => {
                                    let gsum = summaries.get(g.index());
                                    let gmask = gsum.map_or(u64::MAX, |s| s.param_mask);
                                    for a in &accesses {
                                        if let OperandRole::Arg(idx) = a.role
                                            && (idx >= 64 || (gmask & (1u64 << idx)) != 0)
                                        {
                                            match a.place.root {
                                                PlaceRoot::Local(l) if l.index() < deps.len() => {
                                                    call_p |= deps[l.index()];
                                                    call_c |= cap_deps[l.index()];
                                                }
                                                PlaceRoot::Capture(ci) => {
                                                    call_c |=
                                                        if ci < 64 { 1u64 << ci } else { u64::MAX };
                                                }
                                                PlaceRoot::Local(_) => {}
                                            }
                                        }
                                    }
                                    if let Some(gs) = gsum
                                        && !gs.ret_field_param_masks.is_empty()
                                    {
                                        let mut out_fields =
                                            Vec::with_capacity(gs.ret_field_param_masks.len());
                                        for &fmask in &gs.ret_field_param_masks {
                                            let mut fm = 0u64;
                                            for (ai, arg) in args.iter().enumerate() {
                                                if ai >= 64 || (fmask & (1u64 << ai)) != 0 {
                                                    fm |= op_place_deps(arg, &deps);
                                                }
                                            }
                                            out_fields.push(fm);
                                        }
                                        call_fields = FieldFlow::Known(out_fields);
                                    }
                                }
                                Callee::Closure { closure, .. } => {
                                    let cx = body.cx(&module.tys);
                                    let closure_def =
                                        zil_ir::body::type_place(closure, &cx).ok().and_then(|t| {
                                            let ty = t.ty();
                                            if let zil_ir::body::TyKind::Closure { def, .. } =
                                                module.tys.kind(ty)
                                            {
                                                Some(*def)
                                            } else if let zil_ir::body::TyKind::Managed(inner)
                                            | zil_ir::body::TyKind::Ref {
                                                pointee: inner,
                                                ..
                                            } = module.tys.kind(ty)
                                            {
                                                if let zil_ir::body::TyKind::Closure {
                                                    def, ..
                                                } = module.tys.kind(*inner)
                                                {
                                                    Some(*def)
                                                } else {
                                                    None
                                                }
                                            } else {
                                                None
                                            }
                                        });
                                    let csum = closure_def.and_then(|def| {
                                        module
                                            .functions
                                            .iter()
                                            .position(|f| f.body.closure == Some(def))
                                            .and_then(|idx| summaries.get(idx))
                                    });
                                    let pmask = csum.map_or(u64::MAX, |s| s.param_mask);
                                    let cmask = csum.map_or(u64::MAX, |s| s.capture_mask);
                                    // Receiver captures
                                    let mut used_fields = false;
                                    if closure.projections.is_empty()
                                        && let PlaceRoot::Local(rl) = closure.root
                                        && let Some(FieldFlow::Known(rf)) =
                                            local_fields.get(rl.index())
                                    {
                                        let max_bit = if rf.len() >= 64 {
                                            u64::MAX
                                        } else {
                                            (1u64 << rf.len()) - 1
                                        };
                                        if (cmask & !max_bit) == 0 {
                                            used_fields = true;
                                            for (ci, &fm) in rf.iter().enumerate() {
                                                if (cmask & (1u64 << ci)) != 0 {
                                                    call_p |= fm;
                                                }
                                            }
                                        }
                                    }
                                    if !used_fields && cmask != 0 {
                                        match closure.root {
                                            PlaceRoot::Local(rl) if rl.index() < deps.len() => {
                                                call_p |= deps[rl.index()];
                                                call_c |= cap_deps[rl.index()];
                                            }
                                            PlaceRoot::Capture(ci) => {
                                                call_c |=
                                                    if ci < 64 { 1u64 << ci } else { u64::MAX };
                                            }
                                            PlaceRoot::Local(_) => {}
                                        }
                                    }
                                    for a in &accesses {
                                        if let OperandRole::Arg(idx) = a.role
                                            && (idx >= 64 || (pmask & (1u64 << idx)) != 0)
                                        {
                                            match a.place.root {
                                                PlaceRoot::Local(l) if l.index() < deps.len() => {
                                                    call_p |= deps[l.index()];
                                                    call_c |= cap_deps[l.index()];
                                                }
                                                PlaceRoot::Capture(ci) => {
                                                    call_c |=
                                                        if ci < 64 { 1u64 << ci } else { u64::MAX };
                                                }
                                                PlaceRoot::Local(_) => {}
                                            }
                                        }
                                    }
                                }
                                Callee::Foreign(_)
                                | Callee::Indirect(_)
                                | Callee::DynSlot { .. }
                                | Callee::Intrinsic(_) => {
                                    for a in &accesses {
                                        if a.access != PlaceAccess::Init {
                                            match a.place.root {
                                                PlaceRoot::Local(l) if l.index() < deps.len() => {
                                                    call_p |= deps[l.index()];
                                                    call_c |= cap_deps[l.index()];
                                                }
                                                PlaceRoot::Capture(ci) => {
                                                    call_c |=
                                                        if ci < 64 { 1u64 << ci } else { u64::MAX };
                                                }
                                                PlaceRoot::Local(_) => {}
                                            }
                                        }
                                    }
                                }
                            }
                            // Propagate transitive Store/ByteBuffer structural mutations.
                            let mark_op = |op: &Operand, mp: &mut u64, mc: &mut u64| {
                                *mp |= op_place_deps(op, &deps);
                                *mc |= op_place_cap_deps(op, &cap_deps);
                            };
                            let mark_closure_caps =
                                |closure: &zil_ir::body::Place, mp: &mut u64, mc: &mut u64| {
                                    let cx = body.cx(&module.tys);
                                    let closure_def =
                                        zil_ir::body::type_place(closure, &cx).ok().and_then(|t| {
                                            let ty = t.ty();
                                            if let zil_ir::body::TyKind::Closure { def, .. } =
                                                module.tys.kind(ty)
                                            {
                                                Some(*def)
                                            } else if let zil_ir::body::TyKind::Managed(inner)
                                            | zil_ir::body::TyKind::Ref {
                                                pointee: inner,
                                                ..
                                            } = module.tys.kind(ty)
                                            {
                                                if let zil_ir::body::TyKind::Closure {
                                                    def, ..
                                                } = module.tys.kind(*inner)
                                                {
                                                    Some(*def)
                                                } else {
                                                    None
                                                }
                                            } else {
                                                None
                                            }
                                        });
                                    let csum = closure_def.and_then(|def| {
                                        module
                                            .functions
                                            .iter()
                                            .position(|f| f.body.closure == Some(def))
                                            .and_then(|idx| summaries.get(idx))
                                    });
                                    let cmut = csum.map_or(u64::MAX, |s| s.mutating_captures);
                                    if cmut == 0 {
                                        return csum.map_or(u64::MAX, |s| s.mutating_params);
                                    }
                                    let mut used = false;
                                    if closure.projections.is_empty()
                                        && let PlaceRoot::Local(rl) = closure.root
                                        && let (
                                            Some(FieldFlow::Known(rfp)),
                                            Some(FieldFlow::Known(rfc)),
                                        ) = (
                                            local_fields.get(rl.index()),
                                            local_cap_fields.get(rl.index()),
                                        )
                                        && rfp.len() == rfc.len()
                                    {
                                        let max_bit = if rfp.len() >= 64 {
                                            u64::MAX
                                        } else {
                                            (1u64 << rfp.len()) - 1
                                        };
                                        if (cmut & !max_bit) == 0 {
                                            used = true;
                                            for (ci, (&fp, &fc)) in
                                                rfp.iter().zip(rfc.iter()).enumerate()
                                            {
                                                if (cmut & (1u64 << ci)) != 0 {
                                                    *mp |= fp;
                                                    *mc |= fc;
                                                }
                                            }
                                        }
                                    }
                                    if !used {
                                        match closure.root {
                                            PlaceRoot::Local(rl) if rl.index() < deps.len() => {
                                                *mp |= deps[rl.index()];
                                                *mc |= cap_deps[rl.index()];
                                            }
                                            PlaceRoot::Capture(ci) => {
                                                *mc |= if ci < 64 { 1u64 << ci } else { u64::MAX };
                                            }
                                            PlaceRoot::Local(_) => {}
                                        }
                                    }
                                    csum.map_or(u64::MAX, |s| s.mutating_params)
                                };
                            match callee {
                                Callee::Intrinsic(op) if is_structural_mutation(*op) => {
                                    if let Some(a0) = args.first() {
                                        mark_op(a0, &mut mut_param, &mut mut_cap);
                                    }
                                }
                                Callee::Intrinsic(IntrinsicOp::ThreadScope) => {
                                    if let Some(Operand::Copy(p) | Operand::Move(p)) = args.get(2) {
                                        let _ = mark_closure_caps(p, &mut mut_param, &mut mut_cap);
                                    }
                                    if let Some(cell) = args.get(1) {
                                        let cx = body.cx(&module.tys);
                                        let cell_has_desc = match cell {
                                            Operand::Copy(p) | Operand::Move(p) => {
                                                zil_ir::body::type_place(p, &cx).map_or(true, |t| {
                                                    ty_holds_store_or_bytes_desc(
                                                        &module.tys,
                                                        t.ty(),
                                                    )
                                                })
                                            }
                                            Operand::Const(_) | Operand::Pooled(_) => false,
                                        };
                                        if cell_has_desc {
                                            mark_op(cell, &mut mut_param, &mut mut_cap);
                                        }
                                    }
                                }
                                Callee::Direct(g) => {
                                    let gmut = summaries
                                        .get(g.index())
                                        .map_or(u64::MAX, |s| s.mutating_params);
                                    for (ai, arg) in args.iter().enumerate() {
                                        if ai >= 64 || (gmut & (1u64 << ai)) != 0 {
                                            mark_op(arg, &mut mut_param, &mut mut_cap);
                                        }
                                    }
                                }
                                Callee::Closure { closure, .. } => {
                                    let pmut =
                                        mark_closure_caps(closure, &mut mut_param, &mut mut_cap);
                                    for (ai, arg) in args.iter().enumerate() {
                                        if ai >= 64 || (pmut & (1u64 << ai)) != 0 {
                                            mark_op(arg, &mut mut_param, &mut mut_cap);
                                        }
                                    }
                                }
                                Callee::Indirect(_)
                                | Callee::DynSlot { .. }
                                | Callee::Foreign(_) => {
                                    let cx = body.cx(&module.tys);
                                    for arg in args {
                                        if let Operand::Copy(p) | Operand::Move(p) = arg
                                            && zil_ir::body::type_place(p, &cx).map_or(true, |t| {
                                                ty_holds_store_or_bytes_desc(&module.tys, t.ty())
                                            })
                                        {
                                            mark_op(arg, &mut mut_param, &mut mut_cap);
                                        }
                                    }
                                }
                                Callee::Intrinsic(_) => {}
                            }
                            // Propagate transitive address stores through writable parameters/captures.
                            if place_holds_addr(dest)
                                && (dest.first_deref().is_some()
                                    || matches!(dest.root, PlaceRoot::Capture(_)))
                            {
                                match dest.root {
                                    PlaceRoot::Local(l) if l.index() < deps.len() => {
                                        addr_store_param |= deps[l.index()];
                                        addr_store_cap |= cap_deps[l.index()];
                                    }
                                    PlaceRoot::Capture(ci) => {
                                        addr_store_cap |=
                                            if ci < 64 { 1u64 << ci } else { u64::MAX };
                                    }
                                    PlaceRoot::Local(_) => {}
                                }
                            }
                            match callee {
                                Callee::Direct(g) => {
                                    let gas = summaries
                                        .get(g.index())
                                        .map_or(u64::MAX, |s| s.addr_store_params);
                                    for (ai, arg) in args.iter().enumerate() {
                                        if ai >= 64 || (gas & (1u64 << ai)) != 0 {
                                            mark_op(
                                                arg,
                                                &mut addr_store_param,
                                                &mut addr_store_cap,
                                            );
                                        }
                                    }
                                }
                                Callee::Closure { closure, .. } => {
                                    let cx = body.cx(&module.tys);
                                    let closure_def =
                                        zil_ir::body::type_place(closure, &cx).ok().and_then(|t| {
                                            let ty = t.ty();
                                            if let zil_ir::body::TyKind::Closure { def, .. } =
                                                module.tys.kind(ty)
                                            {
                                                Some(*def)
                                            } else if let zil_ir::body::TyKind::Managed(inner)
                                            | zil_ir::body::TyKind::Ref {
                                                pointee: inner,
                                                ..
                                            } = module.tys.kind(ty)
                                            {
                                                if let zil_ir::body::TyKind::Closure {
                                                    def, ..
                                                } = module.tys.kind(*inner)
                                                {
                                                    Some(*def)
                                                } else {
                                                    None
                                                }
                                            } else {
                                                None
                                            }
                                        });
                                    let csum = closure_def.and_then(|def| {
                                        module
                                            .functions
                                            .iter()
                                            .position(|f| f.body.closure == Some(def))
                                            .and_then(|idx| summaries.get(idx))
                                    });
                                    let cas = csum.map_or(u64::MAX, |s| s.addr_store_captures);
                                    if cas != 0 {
                                        match closure.root {
                                            PlaceRoot::Local(rl) if rl.index() < deps.len() => {
                                                addr_store_param |= deps[rl.index()];
                                                addr_store_cap |= cap_deps[rl.index()];
                                            }
                                            PlaceRoot::Capture(ci) => {
                                                addr_store_cap |=
                                                    if ci < 64 { 1u64 << ci } else { u64::MAX };
                                            }
                                            PlaceRoot::Local(_) => {}
                                        }
                                    }
                                    let pas = csum.map_or(u64::MAX, |s| s.addr_store_params);
                                    for (ai, arg) in args.iter().enumerate() {
                                        if ai >= 64 || (pas & (1u64 << ai)) != 0 {
                                            mark_op(
                                                arg,
                                                &mut addr_store_param,
                                                &mut addr_store_cap,
                                            );
                                        }
                                    }
                                }
                                Callee::Intrinsic(IntrinsicOp::StorePush) => {
                                    let elem_holds_addr = match args.get(1) {
                                        Some(Operand::Copy(p) | Operand::Move(p)) => {
                                            place_holds_addr(p)
                                        }
                                        Some(Operand::Const(_) | Operand::Pooled(_)) | None => {
                                            false
                                        }
                                    };
                                    if elem_holds_addr && let Some(a0) = args.first() {
                                        mark_op(a0, &mut addr_store_param, &mut addr_store_cap);
                                    }
                                }
                                Callee::Intrinsic(
                                    IntrinsicOp::ThreadScope | IntrinsicOp::ScopedCallback,
                                )
                                | Callee::Indirect(_)
                                | Callee::DynSlot { .. }
                                | Callee::Foreign(_) => {
                                    for arg in args {
                                        mark_op(arg, &mut addr_store_param, &mut addr_store_cap);
                                    }
                                }
                                Callee::Intrinsic(_) => {}
                            }
                            if dest
                                .projections
                                .iter()
                                .any(|p| matches!(p, Projection::Deref))
                                && (call_p != 0 || call_c != 0)
                            {
                                ret_param |= all_params_mask;
                                ret_cap = u64::MAX;
                                ret_fields = FieldFlow::Top;
                            } else if let PlaceRoot::Local(l) = dest.root
                                && l.index() < deps.len()
                            {
                                let np = deps[l.index()] | call_p;
                                let nc = cap_deps[l.index()] | call_c;
                                if np != deps[l.index()] || nc != cap_deps[l.index()] {
                                    deps[l.index()] = np;
                                    cap_deps[l.index()] = nc;
                                    local_changed = true;
                                }
                                let dest_flow = if dest.projections.is_empty() {
                                    call_fields
                                } else {
                                    FieldFlow::Top
                                };
                                if local_fields[l.index()].join_with(&dest_flow) {
                                    local_changed = true;
                                }
                                if local_cap_fields[l.index()].join_with(&FieldFlow::Top) {
                                    local_changed = true;
                                }
                            }
                        }
                        Terminator::Goto { .. }
                        | Terminator::Switch { .. }
                        | Terminator::Assert { .. }
                        | Terminator::Unreachable
                        | Terminator::Trap(_) => {}
                    }
                }
            }
            let next_field_masks = match &ret_fields {
                FieldFlow::Known(v) => v.clone(),
                FieldFlow::Unset | FieldFlow::Top => Vec::new(),
            };
            if ret_param != summaries[fi].param_mask
                || ret_cap != summaries[fi].capture_mask
                || mut_param != summaries[fi].mutating_params
                || mut_cap != summaries[fi].mutating_captures
                || addr_store_param != summaries[fi].addr_store_params
                || addr_store_cap != summaries[fi].addr_store_captures
                || next_field_masks != summaries[fi].ret_field_param_masks
                || ret_fields != field_flows[fi]
            {
                summaries[fi].param_mask = ret_param;
                summaries[fi].capture_mask = ret_cap;
                summaries[fi].mutating_params = mut_param;
                summaries[fi].mutating_captures = mut_cap;
                summaries[fi].addr_store_params = addr_store_param;
                summaries[fi].addr_store_captures = addr_store_cap;
                summaries[fi].ret_field_param_masks = next_field_masks;
                field_flows[fi] = ret_fields;
                changed = true;
                for &dep in &dependents[fi] {
                    next_dirty[dep] = true;
                }
                if func.body.closure.is_some() {
                    for &cc in &closure_callers {
                        next_dirty[cc] = true;
                    }
                }
            }
        }
        std::mem::swap(&mut dirty, &mut next_dirty);
    }
    summaries
}

/// Verifies that every `DynImpl` method implementing a `DynSlotSig` whose
/// result borrows strictly from trait lifetime parameters (`Region::Param`,
/// no unannotated `Region::Static` references) preserves that fixed-region
/// contract in its own signature and does not bind its receiver `&self` to
/// a returned region parameter.
///
/// Called independently by both `zil-solve` and `zil-check` so forged Raw/Wire
/// modules cannot pair a `&self`-borrowing implementation with a trait-lifetime
/// slot or forge `self: &'a Self` to smuggle `&self.local` past `OwnedDyn` drop.
pub fn check_dyn_impl_region_contract(
    module: &zil_ir::body::Module,
) -> Result<(), (zil_ir::body::FnId, &'static str)> {
    use zil_ir::body::{FnId, Region, TyKind};
    let tys = &module.tys;
    for dimpl in &module.dyn_impls {
        let Some(def) = tys.get_dyn(dimpl.dyn_id) else {
            return Err((FnId(0), "dyn impl references unknown DynId"));
        };
        for (slot, &fn_id) in def.slots.iter().zip(&dimpl.methods) {
            if !tys.dyn_slot_has_fixed_regions(slot.result) {
                continue;
            }
            let Some(func) = module.functions.get(fn_id.index()) else {
                return Err((fn_id, "dyn impl method FnId out of bounds"));
            };
            if func.body.closure.is_some() || !func.body.captures.is_empty() {
                return Err((
                    fn_id,
                    "dyn impl method cannot be a closure or carry captures",
                ));
            }
            if !tys.dyn_slot_has_fixed_regions(func.sig.result) {
                return Err((
                    fn_id,
                    "dyn impl method erases fixed trait-lifetime return contract",
                ));
            }
            if slot.receiver.is_some() {
                let res_regions = tys.regions_in(func.sig.result);
                let Some(&recv_ty) = func.sig.params.first() else {
                    return Err((fn_id, "dyn impl method missing receiver parameter"));
                };
                match tys.get(recv_ty) {
                    Some(TyKind::Ref {
                        region: Region::Param(r),
                        ..
                    }) if res_regions.contains(r) => {
                        return Err((
                            fn_id,
                            "dyn impl method receiver region cannot alias fixed trait-lifetime return",
                        ));
                    }
                    Some(TyKind::Ref { .. }) => {}
                    _ => {
                        return Err((fn_id, "dyn impl method receiver type mismatch"));
                    }
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use zil_ir::body::{
        Block, Body, FnRole, FnSig, Function, Instruction, InstructionKind, IntTy, Local,
        LocalDecl, Module, Mutability, Operand, ParamEnv, Place, PlaceRoot, Projection, Region,
        ResumeMode, Terminator, TyKind, TyTable,
    };

    #[test]
    fn compute_return_summaries_separates_replace_old_and_incoming_provenance() {
        let mut tys = TyTable::new();
        let i32_ty = tys.intern(TyKind::Int(IntTy::I32));
        let ref_i32 = tys.intern(TyKind::Ref {
            region: Region::Static,
            mutability: Mutability::Shared,
            pointee: i32_ty,
        });
        let mut_slot = tys.intern(TyKind::Ref {
            region: Region::Static,
            mutability: Mutability::Exclusive,
            pointee: ref_i32,
        });
        // Closure capturing `slot: &mut &i32` (Capture(0)), taking `incoming: &i32` (Local(0)):
        //   Replace { place: *Capture(0), value: Move(Local(0)), old: Local(1) }
        //   Return Move(Local(1))
        let func = Function {
            name: "swap_capture_return_old".into(),
            sig: FnSig {
                params: vec![ref_i32],
                result: ref_i32,
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
                        ty: ref_i32,
                        name: Some("incoming".into()),
                    },
                    LocalDecl {
                        ty: ref_i32,
                        name: Some("old".into()),
                    },
                ],
                captures: vec![mut_slot],
                closure: None,
                result: ref_i32,
                param_env: ParamEnv::default(),
                drop_flags: vec![],
                blocks: vec![Block {
                    instructions: vec![Instruction {
                        kind: InstructionKind::Replace {
                            place: Place {
                                root: PlaceRoot::Capture(0),
                                projections: vec![Projection::Deref],
                            },
                            value: Operand::Move(Place::local(Local(0))),
                            old: Local(1),
                        },
                        span: Span::default(),
                    }],
                    terminator: Terminator::Return {
                        value: Operand::Move(Place::local(Local(1))),
                    },
                    terminator_span: Span::default(),
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
        let summaries = compute_return_summaries(&module);
        assert_eq!(
            summaries[0].capture_mask, 0b1,
            "old returned from Replace must carry Capture(0) provenance"
        );
        assert_eq!(
            summaries[0].param_mask, 0,
            "old returned from Replace must not inherit OperandRole::Value (param 0) provenance"
        );
    }
}
