//! Persistent type domains are distinct from resource and access authority.
//! A zero-domain proof recursively checks stored/opaque children; owning shells
//! never hide an unknown capture or retained Context behind a Known0 result.
use super::*;

enum Item<'a> {
    Type(&'a Type, usize),
    LeaveClosure(ClosureId),
}
fn shape_empty(shape: &StorageLifeShape) -> bool {
    shape.binder_count == 0
        && shape.slots.is_empty()
        && shape.retained.is_empty()
        && shape.nominal_arguments.is_empty()
}
fn retained_empty(shape: &RetainedLifeShape) -> bool {
    shape.binder_count == 0
        && shape.ports.is_empty()
        && shape.retained.is_empty()
        && shape.nominal_arguments.is_empty()
}
pub(super) fn known_zero(module: &TypedModule, ty: &Type, b: &mut Budget) -> CompileResult<bool> {
    let mut work = vec![Item::Type(ty, 0)];
    let mut active = std::collections::BTreeSet::new();
    while let Some(next) = work.pop() {
        b.spend(1, WORK)?;
        let (ty, depth) = match next {
            Item::LeaveClosure(id) => {
                active.remove(&id);
                continue;
            }
            Item::Type(ty, depth) => (ty, depth),
        };
        if depth > b.partial_depth.min(128) {
            return Err(limit("dynamic actual type domain exceeds depth"));
        }
        match ty {
            Type::BorrowedStorage { storage, life } => {
                if !shape_empty(life) {
                    return Ok(false);
                }
                let limits = zil_base::Limits {
                    max_nodes: b.helper_nodes,
                    max_call_depth: b.partial_depth,
                    ..zil_base::Limits::default()
                };
                crate::stored_flow::qualification(module, ty, &limits, b)?;
                work.push(Item::Type(storage, depth + 1));
            }
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                b.spend(fields.len(), WORK)?;
                work.extend(fields.iter().map(|(_, t)| Item::Type(t, depth + 1)));
            }
            Type::Tuple(fields) => {
                b.spend(fields.len(), WORK)?;
                work.extend(fields.iter().map(|t| Item::Type(t, depth + 1)));
            }
            Type::Enum { variants, .. } => {
                b.spend(variants.len(), WORK)?;
                for (_, fields) in variants {
                    b.spend(fields.len(), WORK)?;
                    work.extend(fields.iter().map(|t| Item::Type(t, depth + 1)));
                }
            }
            Type::Array { element, .. }
            | Type::Managed(element)
            | Type::OwnedStore { element }
            | Type::Opaque {
                hidden: element, ..
            } => work.push(Item::Type(element, depth + 1)),
            Type::ErrorUnion { error, result } => {
                work.push(Item::Type(error, depth + 1));
                work.push(Item::Type(result, depth + 1));
            }
            Type::Closure(id) => {
                b.spend(module.closures.len().saturating_add(active.len()), WORK)?;
                if !active.insert(*id) {
                    return Err(fail("cyclic concrete closure capture domain"));
                }
                let mut matches = module.closures.iter().filter(|c| c.id == *id);
                let closure = matches
                    .next()
                    .ok_or_else(|| fail("closure type has no actual environment definition"))?;
                if matches.next().is_some() {
                    return Err(fail("closure type has ambiguous concrete identity"));
                }
                if closure
                    .storage_life
                    .as_ref()
                    .is_some_and(|life| life.fixed_binder_count != 0)
                {
                    return Ok(false);
                }
                b.spend(closure.captures.len(), WORK)?;
                work.push(Item::LeaveClosure(*id));
                work.extend(closure.captures.iter().map(|t| Item::Type(t, depth + 1)));
            }
            Type::Dyn { interface } | Type::OwnedDyn { interface } => {
                b.spend(module.dyn_interfaces.len(), WORK)?;
                let mut matches = module.dyn_interfaces.iter().filter(|i| i.id == *interface);
                let interface = matches
                    .next()
                    .ok_or_else(|| fail("erased type has no actual interface directory"))?;
                if matches.next().is_some() {
                    return Err(fail("erased type has ambiguous interface identity"));
                }
                if !retained_empty(&interface.retained_life) {
                    return Ok(false);
                }
                crate::dyn_context::validate(module, interface, b)?;
            }
            Type::Task {
                retained_life,
                result,
            } => {
                // This is an explicit public domain. Actual worker creation and
                // complete state/cleanup qualification are separate obligations.
                if !retained_empty(retained_life) {
                    return Ok(false);
                }
                work.push(Item::Type(result, depth + 1));
            }
            Type::FunctionPointer { .. } | Type::ForeignCallback { .. } => {}
            Type::BorrowedFunctionPointer { life, .. } => {
                if life.fixed_binder_count != 0 {
                    return Ok(false);
                }
            }
            Type::ByteBuffer | Type::TaskI32 | Type::ForeignHandle | Type::CWideString => {}
            ty if ty.is_b0_scalar() => {}
            _ => return Ok(false),
        }
    }
    Ok(true)
}

pub(super) fn check(
    module: &TypedModule,
    value: &DynTypeValue,
    fixed: u32,
    invocation: Option<u32>,
    b: &mut Budget,
) -> CompileResult<()> {
    crate::dyn_metadata::recipe(value, b)?;
    if value.classes.len() != value.life.binder_count as usize {
        return Err(fail("type value omits part of its complete local domain"));
    }
    for binder in &value.classes {
        b.spend(1, WORK)?;
        match binder {
            StorageBinder::Fixed(i) if *i < fixed => {}
            StorageBinder::Invocation(i) if invocation.is_some_and(|count| *i < count) => {}
            _ => {
                return Err(fail(
                    "persistent type value uses an absent or wrong-scoped class",
                ));
            }
        }
    }
    match &value.ty {
        Type::BorrowedStorage { life, .. } => {
            if life != &value.life {
                return Err(fail("embedded and external whole type domains differ"));
            }
            let limits = zil_base::Limits {
                max_nodes: b.helper_nodes,
                max_call_depth: b.partial_depth,
                ..zil_base::Limits::default()
            };
            crate::stored_flow::qualification(module, &value.ty, &limits, b)?;
        }
        Type::Address { inner, mutable } if known_zero(module, inner, b)? => {
            let expected = StorageLifeShape {
                binder_count: 1,
                slots: vec![StorageLifeSlot {
                    path: vec![],
                    binder: 0,
                    mode: if *mutable {
                        StoredBorrowMode::ExclusiveAddress
                    } else {
                        StoredBorrowMode::SharedAddress
                    },
                }],
                retained: vec![],
                nominal_arguments: vec![],
            };
            if value.life != expected {
                return Err(fail("raw address loses its actual outer borrow domain"));
            }
        }
        _ if known_zero(module, &value.ty, b)? => {
            if !shape_empty(&value.life) {
                return Err(fail(
                    "Known0 type is retagged with an invented lifetime domain",
                ));
            }
        }
        _ => {
            return Err(fail(
                "complete actual opaque or compound domain has not been established",
            ));
        }
    }
    Ok(())
}
