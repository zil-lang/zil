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
mod state;
mod transfer;

pub use loans::{EXTERNAL, Liveness, Loan, LoanKind, LoanTable};
pub use paths::{MovePaths, PathId, tracked_len};
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
pub fn compute_return_summaries(module: &zil_ir::body::Module) -> Vec<zil_ir::body::ReturnSummary> {
    use zil_ir::body::{
        AggregateKind, Callee, InstructionKind, Operand, OperandRole, PlaceAccess, PlaceRoot,
        Projection, ReturnSummary, Rvalue, Terminator,
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

    let mut changed = true;
    let max_rounds = n.saturating_mul(64).max(1) + 1;
    for _ in 0..max_rounds {
        if !changed {
            break;
        }
        changed = false;
        for (fi, func) in module.functions.iter().enumerate() {
            let body = &func.body;
            let param_count = body.params().count();
            if param_count >= 64 {
                if summaries[fi].param_mask != u64::MAX
                    || summaries[fi].capture_mask != u64::MAX
                    || field_flows[fi] != FieldFlow::Top
                {
                    summaries[fi].param_mask = u64::MAX;
                    summaries[fi].capture_mask = u64::MAX;
                    summaries[fi].ret_field_param_masks.clear();
                    field_flows[fi] = FieldFlow::Top;
                    changed = true;
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
            for (pi, p) in body.params().enumerate() {
                if p.index() < deps.len() {
                    deps[p.index()] = 1u64 << pi;
                    local_fields[p.index()] = FieldFlow::Top;
                }
            }

            let mut local_changed = true;
            let mut ret_param = summaries[fi].param_mask;
            let mut ret_cap = summaries[fi].capture_mask;
            let mut ret_fields = field_flows[fi].clone();

            let cx = body.cx(&module.tys);
            let place_holds_addr = |p: &zil_ir::body::Place| -> bool {
                zil_ir::body::type_place(p, &cx).map_or(true, |t| module.tys.holds_address(t.ty()))
            };

            while local_changed {
                local_changed = false;
                for block in &body.blocks {
                    for inst in &block.instructions {
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
                                    let rhs_flow =
                                        if !dest.projections.is_empty() {
                                            FieldFlow::Top
                                        } else {
                                            match value {
                                                Rvalue::Aggregate {
                                                    kind: AggregateKind::Closure { .. },
                                                    operands,
                                                } => FieldFlow::Known(
                                                    operands
                                                        .iter()
                                                        .map(|f| op_place_deps(f, &deps))
                                                        .collect(),
                                                ),
                                                Rvalue::Use(
                                                    Operand::Copy(p) | Operand::Move(p),
                                                ) if p.projections.is_empty() => match p.root {
                                                    PlaceRoot::Local(src)
                                                        if src.index() < local_fields.len() =>
                                                    {
                                                        local_fields[src.index()].clone()
                                                    }
                                                    PlaceRoot::Local(_) | PlaceRoot::Capture(_) => {
                                                        FieldFlow::Top
                                                    }
                                                },
                                                Rvalue::Use(_)
                                                | Rvalue::Aggregate { .. }
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
                                                | Rvalue::StaticUtf(_) => FieldFlow::Top,
                                            }
                                        };
                                    if local_fields[l.index()].join_with(&rhs_flow) {
                                        local_changed = true;
                                    }
                                }
                            }
                            InstructionKind::Replace { place, old, .. } => {
                                let mut rhs_p = 0u64;
                                let mut rhs_c = 0u64;
                                for a in &accesses {
                                    match a.place.root {
                                        PlaceRoot::Local(l) if l.index() < deps.len() => {
                                            rhs_p |= deps[l.index()];
                                            rhs_c |= cap_deps[l.index()];
                                        }
                                        PlaceRoot::Capture(ci) => {
                                            rhs_c |= if ci < 64 { 1u64 << ci } else { u64::MAX };
                                        }
                                        PlaceRoot::Local(_) => {}
                                    }
                                }
                                if let PlaceRoot::Local(l) = place.root
                                    && l.index() < deps.len()
                                {
                                    let np = deps[l.index()] | rhs_p;
                                    let nc = cap_deps[l.index()] | rhs_c;
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
                                    let np = deps[old.index()] | rhs_p;
                                    let nc = cap_deps[old.index()] | rhs_c;
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
                || next_field_masks != summaries[fi].ret_field_param_masks
                || ret_fields != field_flows[fi]
            {
                summaries[fi].param_mask = ret_param;
                summaries[fi].capture_mask = ret_cap;
                summaries[fi].ret_field_param_masks = next_field_masks;
                field_flows[fi] = ret_fields;
                changed = true;
            }
        }
    }
    summaries
}
