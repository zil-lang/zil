//! Derive callback dependency uppers from complete actual capture types.
//! These bounds establish neither a target nor authority to access it.
use super::*;

pub(super) fn bounds(
    module: &TypedModule,
    root: &Type,
    b: &mut Budget,
) -> Check<(bool, bool, bool)> {
    let mut result = (false, false, false);
    let mut pending = vec![(root, 0usize)];
    while let Some((ty, depth)) = pending.pop() {
        b.spend(1)?;
        if depth > b.limits.max_call_depth {
            return Err(limit("callback capture bound depth exceeded"));
        }
        match ty {
            Type::Address { mutable, .. } => {
                result.0 = true;
                result.1 = true;
                result.2 |= *mutable;
            }
            Type::Desc(_) => {
                result.1 = true;
                result.2 = true;
            }
            Type::BorrowedStorage { life, storage } => {
                b.spend(life.slots.len().saturating_add(life.retained.len()))?;
                for slot in &life.slots {
                    result.0 |= matches!(
                        slot.mode,
                        StoredBorrowMode::SharedAddress | StoredBorrowMode::ExclusiveAddress
                    );
                    result.1 = true;
                    result.2 |= matches!(
                        slot.mode,
                        StoredBorrowMode::ExclusiveAddress | StoredBorrowMode::WritableDescription
                    );
                }
                // Physical direct addresses are covered by the exact slot
                // modes above. Opaque children require their own real scopes.
                if !life.retained.is_empty() {
                    pending.push((storage, depth + 1));
                }
            }
            Type::Closure(id) => {
                let d = module.closures.get(id.0 as usize).ok_or_else(|| {
                    reject(
                        "E-VERIFY-CALLABLE-CONTRACT",
                        None,
                        "capture child definition is absent",
                    )
                })?;
                b.spend(d.captures.len())?;
                pending.extend(d.captures.iter().map(|ty| (ty, depth + 1)));
            }
            Type::Task { retained_life, .. } | Type::MutexGuard { retained_life, .. } => {
                b.spend(retained_life.ports.len())?;
                for port in &retained_life.ports {
                    result.0 = true;
                    result.1 = true;
                    result.2 |= matches!(
                        port.mode,
                        StoredBorrowMode::ExclusiveAddress | StoredBorrowMode::WritableDescription
                    );
                }
            }
            Type::Dyn { interface } | Type::OwnedDyn { interface } => {
                b.spend(module.dyn_interfaces.len())?;
                let actual = module
                    .dyn_interfaces
                    .iter()
                    .find(|item| item.id == *interface)
                    .ok_or_else(|| {
                        reject(
                            "E-VERIFY-CALLABLE-CONTRACT",
                            None,
                            "capture dyn interface is absent",
                        )
                    })?;
                b.spend(actual.retained_life.ports.len())?;
                for port in &actual.retained_life.ports {
                    result.0 = true;
                    result.1 = true;
                    result.2 |= matches!(
                        port.mode,
                        StoredBorrowMode::ExclusiveAddress | StoredBorrowMode::WritableDescription
                    );
                }
            }
            Type::Managed(inner)
            | Type::Registration { environment: inner }
            | Type::AsyncRegistration { environment: inner } => {
                // A resource's Copy payload has no external ports. Pollable
                // or callback payloads retain their real environment domains.
                if matches!(
                    inner.as_ref(),
                    Type::Task { .. }
                        | Type::Registration { .. }
                        | Type::AsyncRegistration { .. }
                        | Type::Closure(_)
                ) {
                    pending.push((inner, depth + 1));
                }
            }
            Type::Tuple(children) => {
                b.spend(children.len())?;
                pending.extend(children.iter().map(|ty| (ty, depth + 1)));
            }
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                b.spend(fields.len())?;
                pending.extend(fields.iter().map(|(_, ty)| (ty, depth + 1)));
            }
            Type::Enum { variants, .. } => {
                b.spend(variants.len())?;
                for (_, fields) in variants {
                    b.spend(fields.len())?;
                    pending.extend(fields.iter().map(|ty| (ty, depth + 1)));
                }
            }
            Type::ErrorUnion { error, result } => {
                b.spend(2)?;
                pending.extend([(error.as_ref(), depth + 1), (result.as_ref(), depth + 1)]);
            }
            Type::Array { element, .. }
            | Type::Opaque {
                hidden: element, ..
            } => pending.push((element, depth + 1)),
            _ => {}
        }
    }
    Ok(result)
}
