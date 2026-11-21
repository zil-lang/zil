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

/// Computes a sound, deterministic per-function bitmask of which parameter
/// indices (`0..64`) may flow into each function's return value.
///
/// Both `zil-solve` and `zil-check` derive this independently from the IR
/// module (never from untrusted certificate claims), so a forged certificate
/// cannot weaken caller borrow checking.
pub fn compute_return_param_masks(module: &zil_ir::body::Module) -> Vec<u64> {
    use zil_ir::body::{
        Callee, InstructionKind, OperandRole, PlaceAccess, PlaceRoot, Projection, Terminator,
    };

    let n = module.functions.len();
    let mut masks = vec![0u64; n];
    // Start conservatively for functions with >= 64 params or opaque indirect stores.
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
                if masks[fi] != u64::MAX {
                    masks[fi] = u64::MAX;
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
            for (pi, p) in body.params().enumerate() {
                if p.index() < deps.len() {
                    deps[p.index()] = 1u64 << pi;
                }
            }
            // Local monotone fixpoint over the body.
            let mut local_changed = true;
            let mut ret_mask = masks[fi];
            while local_changed {
                local_changed = false;
                for block in &body.blocks {
                    for inst in &block.instructions {
                        let accesses = inst.kind.accesses();
                        match &inst.kind {
                            InstructionKind::Assign { dest, .. } => {
                                let mut rhs = 0u64;
                                for a in &accesses {
                                    if a.access != PlaceAccess::Init
                                        && let PlaceRoot::Local(l) = a.place.root
                                        && l.index() < deps.len()
                                    {
                                        rhs |= deps[l.index()];
                                    }
                                }
                                if dest
                                    .projections
                                    .iter()
                                    .any(|p| matches!(p, Projection::Deref))
                                {
                                    // Writing through a deref can alias any parameter carrying
                                    // the target address; conservatively taint all params.
                                    if rhs != 0 {
                                        for d in &mut deps {
                                            let next = *d | rhs;
                                            if next != *d {
                                                *d = next;
                                                local_changed = true;
                                            }
                                        }
                                    }
                                } else if let PlaceRoot::Local(l) = dest.root
                                    && l.index() < deps.len()
                                {
                                    let next = deps[l.index()] | rhs;
                                    if next != deps[l.index()] {
                                        deps[l.index()] = next;
                                        local_changed = true;
                                    }
                                }
                            }
                            InstructionKind::Replace { place, old, .. } => {
                                let mut rhs = 0u64;
                                for a in &accesses {
                                    if let PlaceRoot::Local(l) = a.place.root
                                        && l.index() < deps.len()
                                    {
                                        rhs |= deps[l.index()];
                                    }
                                }
                                if let PlaceRoot::Local(l) = place.root
                                    && l.index() < deps.len()
                                {
                                    let next = deps[l.index()] | rhs;
                                    if next != deps[l.index()] {
                                        deps[l.index()] = next;
                                        local_changed = true;
                                    }
                                }
                                if old.index() < deps.len() {
                                    let next = deps[old.index()] | rhs;
                                    if next != deps[old.index()] {
                                        deps[old.index()] = next;
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
                        Terminator::Return { .. } => {
                            for a in &accesses {
                                if let PlaceRoot::Local(l) = a.place.root
                                    && l.index() < deps.len()
                                {
                                    ret_mask |= deps[l.index()];
                                }
                            }
                        }
                        Terminator::Call { callee, dest, .. } => {
                            let mut call_rhs = 0u64;
                            match callee {
                                Callee::Direct(g) => {
                                    let gmask = masks.get(g.index()).copied().unwrap_or(u64::MAX);
                                    for a in &accesses {
                                        if let OperandRole::Arg(idx) = a.role
                                            && (idx >= 64 || (gmask & (1u64 << idx)) != 0)
                                            && let PlaceRoot::Local(l) = a.place.root
                                            && l.index() < deps.len()
                                        {
                                            call_rhs |= deps[l.index()];
                                        }
                                    }
                                }
                                Callee::Foreign(_)
                                | Callee::Indirect(_)
                                | Callee::Closure { .. }
                                | Callee::DynSlot { .. }
                                | Callee::Intrinsic(_) => {
                                    for a in &accesses {
                                        if a.access != PlaceAccess::Init
                                            && let PlaceRoot::Local(l) = a.place.root
                                            && l.index() < deps.len()
                                        {
                                            call_rhs |= deps[l.index()];
                                        }
                                    }
                                }
                            }
                            if let PlaceRoot::Local(l) = dest.root
                                && l.index() < deps.len()
                            {
                                let next = deps[l.index()] | call_rhs;
                                if next != deps[l.index()] {
                                    deps[l.index()] = next;
                                    local_changed = true;
                                }
                            } else if dest
                                .projections
                                .iter()
                                .any(|p| matches!(p, Projection::Deref))
                                && call_rhs != 0
                            {
                                ret_mask |= all_params_mask;
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
            if ret_mask != masks[fi] {
                masks[fi] = ret_mask;
                changed = true;
            }
        }
    }
    masks
}
