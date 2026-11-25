//! Structural module checks: `Drop` binding uniqueness and `dyn Trait` vtable authorization.

use zil_ir::body::{AdtId, AdtKind, FnId, FnRole, Module, TyKind};

use crate::{CheckError, CheckErrorKind};

/// `impl Drop` bindings (`_zil_design.md` §7): each cleanup entry belongs
/// to a resource declared with a user drop, takes exactly that resource's
/// descriptor and returns unit; every such resource has exactly one entry.
pub(crate) fn check_drop_bindings(module: &Module) -> Result<(), CheckError> {
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
pub(crate) fn check_dyn_impls(module: &Module) -> Result<(), CheckError> {
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
    zil_semantics::check_dyn_impl_region_contract(module)
        .map_err(|(fid, reason)| fail(fid, reason))?;
    Ok(())
}
