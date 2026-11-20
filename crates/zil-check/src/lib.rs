//! Checker: the only authority. It never trusts a candidate conclusion; it
//! applies the semantic kernel once per operation and edge to the untrusted
//! certificate and discharges every obligation of
//! `spec/semantic-kernel.md` §6 that the implemented families cover.
#![forbid(unsafe_code)]
#![deny(clippy::wildcard_enum_match_arm)]

mod certificate;

pub use certificate::{CheckBudget, CheckError, CheckErrorKind, check_function};

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
    let fn_sigs = module.fn_sigs();
    let sigs = Sigs {
        functions: &fn_sigs,
        foreigns: &module.foreigns,
        static_count: module.statics.len(),
    };
    let mut remaining = budget;
    if certificate.functions.len() != module.functions.len() {
        return Err(CheckError {
            function: FnId(0),
            kind: CheckErrorKind::CertificateShape("one function certificate per function"),
        });
    }
    for (i, function) in module.functions.iter().enumerate() {
        let id = FnId::from_index(i);
        let cert = &certificate.functions[i];
        if cert.function != id {
            return Err(CheckError {
                function: id,
                kind: CheckErrorKind::CertificateShape("function certificates out of order"),
            });
        }
        check_function(&function.body, &module.tys, sigs, id, cert, &mut remaining)?;
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
