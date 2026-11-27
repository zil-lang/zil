//! Checker: the only authority. It never trusts a candidate conclusion; it
//! applies the semantic kernel once per operation and edge to the untrusted
//! certificate and discharges every obligation of
//! `spec/semantic-kernel.md` §6 that the implemented families cover.
#![forbid(unsafe_code)]
#![deny(clippy::wildcard_enum_match_arm)]

mod certificate;
mod contracts;
pub mod ct_original;
pub mod fidelity;
mod provenance;
pub mod scc;
mod state_protocol;
mod structural;
mod suspend;

pub use certificate::{CheckBudget, CheckError, CheckErrorKind, check_function};
pub use fidelity::{FidelityError, check_fidelity};
pub use scc::{CallScc, SccContractError, call_graph_sccs, validate_scc_contracts};
pub use zil_semantics::{Violation, ViolationKind};

use zil_ir::body::{Certificate, FnId, Module, Sigs};

use crate::contracts::check_contracts;
use crate::structural::{check_drop_bindings, check_dyn_impls};
use crate::suspend::check_suspend_effects;

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
    let ordered_ids: Vec<FnId> = sccs
        .iter()
        .flat_map(|scc| scc.members.iter().copied())
        .collect();
    let workers = std::thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(1)
        .clamp(1, 16)
        .min(ordered_ids.len().max(1));

    if workers <= 1 || ordered_ids.len() < 16 {
        for id in ordered_ids {
            let function = &module.functions[id.index()];
            let cert = &certificate.functions[id.index()];
            check_function(&function.body, &module.tys, sigs, id, cert, &mut remaining)?;
        }
        return Ok(VerifiedModule { module });
    }

    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};

    struct CheckSlot {
        result: Result<(), CheckError>,
        used: u64,
        budget_at_start: u64,
    }

    let n = ordered_ids.len();
    let shared_budget = AtomicU64::new(budget.steps);
    let any_failed = AtomicBool::new(false);
    let next_idx = AtomicUsize::new(0);
    let slots: Vec<Mutex<Option<CheckSlot>>> = (0..n).map(|_| Mutex::new(None)).collect();

    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                loop {
                    if any_failed.load(Ordering::Relaxed) {
                        break;
                    }
                    let pos = next_idx.fetch_add(1, Ordering::Relaxed);
                    if pos >= n {
                        break;
                    }
                    let avail = shared_budget.load(Ordering::Relaxed);
                    if avail == 0 {
                        break;
                    }
                    let id = ordered_ids[pos];
                    let function = &module.functions[id.index()];
                    let cert = &certificate.functions[id.index()];
                    let mut local_budget = CheckBudget { steps: avail };
                    let result = check_function(
                        &function.body,
                        &module.tys,
                        sigs,
                        id,
                        cert,
                        &mut local_budget,
                    );
                    let used = avail.saturating_sub(local_budget.steps);
                    let mut cur = shared_budget.load(Ordering::Relaxed);
                    while let Err(next) = shared_budget.compare_exchange_weak(
                        cur,
                        cur.saturating_sub(used),
                        Ordering::Relaxed,
                        Ordering::Relaxed,
                    ) {
                        cur = next;
                    }
                    if result.is_err() {
                        any_failed.store(true, Ordering::Relaxed);
                    }
                    if let Ok(mut guard) = slots[pos].lock() {
                        *guard = Some(CheckSlot {
                            result,
                            used,
                            budget_at_start: avail,
                        });
                    }
                }
            });
        }
    });

    // Deterministic replay in exact SCC order so the first failing function and
    // budget exhaustion match sequential execution bit-for-bit.
    for (pos, slot_mutex) in slots.into_iter().enumerate() {
        let id = ordered_ids[pos];
        let slot = slot_mutex.into_inner().ok().flatten();
        match slot {
            Some(CheckSlot {
                result: Ok(()),
                used,
                ..
            }) if used <= remaining.steps => {
                remaining.steps -= used;
            }
            Some(CheckSlot {
                result: Err(e),
                used,
                ..
            }) if !matches!(e.kind, CheckErrorKind::ResourceLimit) && used <= remaining.steps => {
                return Err(e);
            }
            Some(CheckSlot {
                result: Err(e),
                budget_at_start,
                ..
            }) if matches!(e.kind, CheckErrorKind::ResourceLimit)
                && budget_at_start >= remaining.steps =>
            {
                return Err(e);
            }
            _ => {
                let function = &module.functions[id.index()];
                let cert = &certificate.functions[id.index()];
                check_function(&function.body, &module.tys, sigs, id, cert, &mut remaining)?;
            }
        }
    }
    Ok(VerifiedModule { module })
}
