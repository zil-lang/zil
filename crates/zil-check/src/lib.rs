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
    for scc in &sccs {
        for &id in &scc.members {
            let function = &module.functions[id.index()];
            let cert = &certificate.functions[id.index()];
            check_function(&function.body, &module.tys, sigs, id, cert, &mut remaining)?;
        }
    }
    Ok(VerifiedModule { module })
}
