//! Producer-only finite recursive Store qualification. Dynamic manager paths,
//! initialized prefixes and loans are checked by the actual CFG consumers.
use super::{Budget, CompileResult, Diagnostic, Phase, bad};
use std::collections::BTreeSet;
use zil_ir::{Type, TypedModule};

const WORK: &str = "recursive Store actual ownership";
fn unsupported(message: &str) -> Vec<Diagnostic> {
    vec![Diagnostic::unsupported(Phase::Analyze, message, None)]
}

/// Scheduling only. Callers have already prepaid/bounded the complete tree.
pub(super) fn selected(element: &Type) -> bool {
    match element {
        Type::Managed(_) => true,
        Type::Package { fields, .. } => fields.iter().any(|(_, field)| match field {
            Type::Managed(payload) => matches!(payload.as_ref(), Type::OwnedStore { .. }),
            Type::Package { .. } => selected(field),
            _ => false,
        }),
        _ => false,
    }
}
fn checked(value: Option<u64>) -> CompileResult<u64> {
    value
        .filter(|n| *n <= i64::MAX as u64)
        .ok_or_else(|| unsupported("recursive Store layout exceeds its target address range"))
}
fn align(size: u64, alignment: u64) -> CompileResult<u64> {
    checked(size.checked_add(alignment - 1)).map(|n| n / alignment * alignment)
}
fn fields<'a>(
    entries: impl Iterator<Item = &'a Type>,
    module: &TypedModule,
    b: &mut Budget,
) -> CompileResult<(u64, u64)> {
    let (mut size, mut maximum) = (0u64, 1u64);
    for ty in entries {
        let (bytes, alignment) = value(ty, module, b)?;
        size = checked(align(size, alignment)?.checked_add(bytes))?;
        maximum = maximum.max(alignment);
    }
    Ok((align(size, maximum)?, maximum))
}
fn identity(name: &str, entries: &[(String, Type)], b: &mut Budget) -> CompileResult<()> {
    b.spend(name.len().saturating_add(entries.len()), WORK)?;
    if name.is_empty() {
        return Err(bad("recursive Store nominal identity is empty"));
    }
    let mut names = BTreeSet::new();
    for (name, _) in entries {
        b.product(
            name.len().saturating_add(1),
            entries.len().saturating_add(1),
            1,
            WORK,
        )?;
        if name.is_empty() || !names.insert(name) {
            return Err(bad("recursive Store field identity is empty or repeated"));
        }
    }
    Ok(())
}
fn resource(
    module: &TypedModule,
    name: &str,
    payload: &Type,
    sealed: bool,
    b: &mut Budget,
) -> CompileResult<()> {
    let mut found = 0usize;
    b.spend(module.resources.len(), WORK)?;
    for r in &module.resources {
        b.spend(
            r.name.len().saturating_add(name.len()).saturating_add(1),
            WORK,
        )?;
        let related = matches!((&r.payload,payload),(Type::Record{name:a,..},Type::Record{name:c,..}) if a==c)
            || matches!((&r.payload,payload),(Type::OwnedStore{element:a},Type::OwnedStore{element:c}) if a==c)
            || r.payload == Type::ByteBuffer && *payload == Type::ByteBuffer;
        if r.name != name && !related {
            continue;
        }
        b.type_work(&r.payload, true, WORK)?;
        b.type_work(payload, true, WORK)?;
        if r.name != name || r.payload != *payload || sealed && r.teardown.is_some() {
            return Err(bad(
                "recursive Store requires the complete exact resource definition",
            ));
        }
        found += 1;
    }
    if found != 1 {
        return Err(bad(
            "recursive Store requires one actual canonical resource definition",
        ));
    }
    Ok(())
}
fn value(ty: &Type, module: &TypedModule, b: &mut Budget) -> CompileResult<(u64, u64)> {
    b.spend(1, WORK)?;
    Ok(match ty {
        Type::Unit => (0, 1),
        Type::Bool | Type::U8 | Type::I8 => (1, 1),
        Type::U16 | Type::I16 => (2, 2),
        Type::U32 | Type::I32 | Type::F32 => (4, 4),
        Type::I64 | Type::U64 | Type::Usize | Type::Isize | Type::F64 => (8, 8),
        Type::Managed(payload) => match payload.as_ref() {
            Type::ByteBuffer => {
                resource(module, zil_ir::BYTE_BUFFER_RESOURCE_NAME, payload, true, b)?;
                (24, 8)
            }
            Type::OwnedStore { element } => {
                carrier(module, element, b)?;
                (24, 8)
            }
            Type::Record {
                name,
                fields: entries,
            } => {
                if !payload.is_copy_value() {
                    return Err(unsupported(
                        "recursive Store resource payload must be complete finite Copy data",
                    ));
                }
                identity(name, entries, b)?;
                resource(module, name, payload, false, b)?;
                fields(entries.iter().map(|(_, ty)| ty), module, b)?
            }
            _ => {
                return Err(unsupported(
                    "recursive Store cannot implicitly close a Task or asynchronous resource",
                ));
            }
        },
        Type::Record {
            name,
            fields: entries,
        }
        | Type::Package {
            name,
            fields: entries,
        } => {
            identity(name, entries, b)?;
            fields(entries.iter().map(|(_, ty)| ty), module, b)?
        }
        Type::Tuple(entries) => fields(entries.iter(), module, b)?,
        Type::Array { element, length } => {
            let (bytes, alignment) = value(element, module, b)?;
            (checked(bytes.checked_mul(u64::from(*length)))?, alignment)
        }
        Type::Enum { name, variants } if ty.is_copy_value() => {
            b.spend(name.len().saturating_add(variants.len()), WORK)?;
            if name.is_empty() || variants.is_empty() {
                return Err(bad("recursive Store enum identity is incomplete"));
            }
            let mut names = BTreeSet::new();
            let (mut maximum, mut alignment) = (0u64, 4u64);
            for (name, entries) in variants {
                b.product(
                    name.len().saturating_add(1),
                    variants.len().saturating_add(1),
                    1,
                    WORK,
                )?;
                if name.is_empty() || !names.insert(name) {
                    return Err(bad("recursive Store variants are empty or repeated"));
                }
                let (bytes, a) = fields(entries.iter(), module, b)?;
                maximum = maximum.max(bytes);
                alignment = alignment.max(a);
            }
            (
                align(
                    checked(align(4, alignment)?.checked_add(maximum))?,
                    alignment,
                )?,
                alignment,
            )
        }
        Type::ErrorUnion { error, result } if ty.is_copy_value() => {
            let (e, ea) = value(error, module, b)?;
            let (r, ra) = value(result, module, b)?;
            let alignment = 4u64.max(ea).max(ra);
            (
                align(
                    checked(align(4, alignment)?.checked_add(e.max(r)))?,
                    alignment,
                )?,
                alignment,
            )
        }
        _ => {
            return Err(unsupported(
                "recursive Store elements need complete owning heads and closed finite inline data",
            ));
        }
    })
}

/// The additional prefix-push error has one owning payload and Unit.
/// Existing Copy-error/Managed-success paths keep their original fees.
/// Tasks and asynchronous heads retain their independent state contracts.
pub(super) fn protocol_payload(ty: &Type) -> Option<&Type> {
    let Type::ErrorUnion { error, result } = ty else {
        return None;
    };
    if **result != Type::Unit {
        return None;
    }
    let owner = error.as_ref();
    matches!(owner,Type::Managed(payload) if matches!(payload.as_ref(),Type::ByteBuffer|Type::OwnedStore{..}|Type::Record{..}))
        .then_some(owner)
}
pub(super) fn protocol(module: &TypedModule, ty: &Type, b: &mut Budget) -> CompileResult<()> {
    let owner = protocol_payload(ty)
        .ok_or_else(|| bad("owning prefix sum must have one real Managed payload and Unit"))?;
    b.type_work(owner, true, WORK)?;
    value(owner, module, b)?;
    Ok(())
}

pub(super) fn carrier(module: &TypedModule, element: &Type, b: &mut Budget) -> CompileResult<()> {
    b.type_work(element, true, WORK)?;
    if !module.target.is_supported_x64_llvm() {
        return Err(unsupported(
            "recursive Store needs a checked x64 LLVM target",
        ));
    }
    // Leaf callback and legacy element families retain their complete existing
    // proofs; a nested manager does not grant a new closure-capture profile.
    if !selected(element) {
        if matches!(element, Type::Package { .. }) {
            b.type_work(element, true, WORK)?;
            let owned = Type::Managed(Box::new(Type::OwnedStore {
                element: Box::new(element.clone()),
            }));
            return super::dynamic_store::ordinary_owned(module, &owned, b);
        }
        return super::pod_store::carrier(module, element, b);
    }
    let (bytes, _) = value(element, module, b)?;
    if bytes == 0 {
        return Err(unsupported(
            "recursive Store needs nonzero actual element storage",
        ));
    }
    b.spend(96, WORK)?;
    let name = match element {
        Type::Managed(_) => format!("@builtin:OwnedStore<owner:{}>", element.fingerprint()),
        Type::Package { name, .. } => format!("@builtin:OwnedStore<{name}>"),
        _ => {
            return Err(bad(
                "recursive Store element selection disagrees with its actual type",
            ));
        }
    };
    b.type_work(element, true, WORK)?;
    let payload = Type::OwnedStore {
        element: Box::new(element.clone()),
    };
    resource(module, &name, &payload, true, b)?;
    // This schedules the existing actual-pointer CFG lane. It grants no
    // Option tag, heap source, ownership state or permission by itself.
    b.nested_store_windows = true;
    b.store_option_results = true;
    b.head_descriptions = true;
    Ok(())
}

/// Actual descriptor effects; callers still prove the source, mode and every
/// inherited loan. A plain Desc parameter never supplies writable authority.
pub(super) fn head_access(kind: &zil_ir::InstructionKind) -> Option<(zil_ir::ValueId, bool)> {
    use zil_ir::InstructionKind as I;
    match *kind {
        I::OwnedStoreReserve { store, .. }
        | I::OwnedStorePush { store, .. }
        | I::OwnedStorePop { store }
        | I::OwnedStoreBorrowMut { store, .. } => Some((store, true)),
        I::OwnedStoreLen { store }
        | I::OwnedStoreCapacity { store }
        | I::OwnedStoreBorrow { store, .. } => Some((store, false)),
        I::ReadDesc { desc, .. } => Some((desc, false)),
        I::WriteDesc { desc, .. } => Some((desc, true)),
        I::ByteBufferReserve { buffer, .. }
        | I::ByteBufferPush { buffer, .. }
        | I::ByteBufferTruncate { buffer, .. } => Some((buffer, true)),
        I::ByteBufferLen { buffer } | I::ByteBufferCapacity { buffer } => Some((buffer, false)),
        I::ByteBufferBorrow { buffer, mutable } => Some((buffer, mutable)),
        _ => None,
    }
}
