//! Sealed predicates are obligations over actual semantic types. They never
//! erase a borrow domain, grant an access, or retire a live cleanup debt.
use super::*;

pub(super) fn check(
    module: &TypedModule,
    value: &terms::Value,
    bound: DynStructuralBound,
    b: &mut Budget,
) -> Check<()> {
    dynamic_metadata::recipe(&value.raw, b)?;
    if bound == DynStructuralBound::StoreElement {
        return dynamic_store::check_store_element(module, &value.raw.ty, None, b);
    }
    let mut active = vec![];
    walk(module, &value.raw.ty, bound, &mut active, b, 0)
}
fn walk(
    module: &TypedModule,
    ty: &Type,
    bound: DynStructuralBound,
    active: &mut Vec<ClosureId>,
    b: &mut Budget,
    depth: usize,
) -> Check<()> {
    b.spend(1)?;
    if depth > b.limits.max_call_depth.min(128) {
        return Err(limit("structural actual type depth exceeded"));
    }
    match ty {
        Type::BorrowedStorage { storage, .. } => walk(module, storage, bound, active, b, depth + 1),
        Type::Address { .. } | Type::Desc(_) => {
            if bound == DynStructuralBound::Copy {
                return Err(bad(
                    "borrowed storage is outside the sealed finite Copy field obligation",
                ));
            }
            // Dropping the head never drops or copies its pointee.
            Ok(())
        }
        Type::Tuple(fields) => {
            b.spend(fields.len())?;
            for field in fields {
                walk(module, field, bound, active, b, depth + 1)?;
            }
            Ok(())
        }
        Type::Record { fields, .. } | Type::Package { fields, .. } => {
            if bound == DynStructuralBound::Copy && matches!(ty, Type::Package { .. }) {
                return Err(bad("affine Package cannot acquire Copy from its fields"));
            }
            b.spend(fields.len())?;
            for (_, field) in fields {
                walk(module, field, bound, active, b, depth + 1)?;
            }
            Ok(())
        }
        Type::Enum { variants, .. } => {
            b.spend(variants.len())?;
            for (_, fields) in variants {
                b.spend(fields.len())?;
                for field in fields {
                    walk(module, field, bound, active, b, depth + 1)?;
                }
            }
            Ok(())
        }
        Type::Array { element, .. } => walk(module, element, bound, active, b, depth + 1),
        Type::ErrorUnion { error, result } => {
            walk(module, error, bound, active, b, depth + 1)?;
            walk(module, result, bound, active, b, depth + 1)
        }
        Type::Managed(payload) => {
            if bound == DynStructuralBound::Copy {
                return Err(bad("an owning actual resource cannot satisfy Copy"));
            }
            resources::check_type_for(module, ty, None, b)?;
            if bound == DynStructuralBound::SyncDrop {
                match payload.as_ref() {
                    Type::TaskI32 | Type::Task { .. } | Type::AsyncRegistration { .. } => {
                        return Err(bad(
                            "Task or async registration has no type-wide synchronous Drop obligation",
                        ));
                    }
                    Type::Registration { environment } => {
                        walk(module, environment, bound, active, b, depth + 1)?
                    }
                    _ => {}
                }
            }
            Ok(())
        }
        Type::Closure(id) => {
            if bound == DynStructuralBound::Copy {
                return Err(bad(
                    "concrete closure identity has no structural Copy admission",
                ));
            }
            b.spend(module.closures.len().saturating_add(active.len()))?;
            if active.contains(id) {
                return Err(bad(
                    "closure cleanup obligation recursively depends on itself",
                ));
            }
            let mut matches = module.closures.iter().filter(|d| d.id == *id);
            let d = matches
                .next()
                .ok_or_else(|| bad("structural closure definition is absent"))?;
            if matches.next().is_some() {
                return Err(bad("structural closure definition is ambiguous"));
            }
            // The complete actual closure inventory is separately validated by
            // values::check, including its entry and free Fixed/static domain.
            active.push(*id);
            b.spend(d.captures.len())?;
            for capture in &d.captures {
                walk(module, capture, bound, active, b, depth + 1)?;
            }
            active.pop();
            Ok(())
        }
        Type::OwnedDyn { .. } => {
            if bound == DynStructuralBound::Copy {
                return Err(bad("erased owning value cannot satisfy Copy"));
            }
            if bound == DynStructuralBound::SyncDrop {
                let Type::OwnedDyn { interface } = ty else {
                    unreachable!()
                };
                b.spend(module.dyn_interfaces.len())?;
                let mut rows = module
                    .dyn_interfaces
                    .iter()
                    .filter(|row| row.id == *interface);
                let actual = rows
                    .next()
                    .ok_or_else(|| bad("erased owner cleanup interface is absent"))?;
                if rows.next().is_some() {
                    return Err(bad("erased owner cleanup interface is ambiguous"));
                }
                // This upper has no Suspend/Spawn or asynchronous operation.
                // Every real erasure still needs the payload/glue CFG proof.
                dynamic_cleanup::bound(actual.owning_cleanup.as_ref(), b)?;
            }
            Ok(())
        }
        Type::FunctionPointer { .. } | Type::ForeignCallback { .. }
            if bound != DynStructuralBound::Copy =>
        {
            Ok(())
        }
        ty if ty.is_copy_value() => Ok(()),
        _ => Err(bad(
            "actual type has no complete finite structural value qualification",
        )),
    }
}
