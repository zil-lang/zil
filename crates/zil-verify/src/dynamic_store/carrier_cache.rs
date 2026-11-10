//! Same-request admission of pre-registered immutable module carrier nodes.
use super::*;
pub(crate) struct Cache {
    module: usize,
    // false is only a bounded actual-node inventory, never a permission.
    entries: BTreeMap<usize, bool>,
}
fn lookup(b: &mut Budget, len: usize) -> Check<()> {
    b.spend(len.checked_ilog2().map_or(1, |n| n as usize + 2))
}
fn register(module: &TypedModule, ty: &Type, b: &mut Budget) -> Check<()> {
    b.spend(1)?;
    let module_key = module as *const TypedModule as usize;
    if b.pod_carriers
        .as_ref()
        .is_some_and(|c| c.module != module_key)
    {
        return Ok(());
    }
    let len = b.pod_carriers.as_ref().map_or(0, |c| c.entries.len());
    lookup(b, len)?;
    let key = ty as *const Type as usize;
    if b.pod_carriers
        .as_ref()
        .is_some_and(|c| c.entries.contains_key(&key))
    {
        return Ok(());
    }
    if b.pod_carriers.is_none() {
        b.spend(2)?;
    }
    lookup(b, len)?; // insertion is a second actual BTree search
    b.spend(3)?; // actual key/value record and eventual tree release before publication
    b.pod_carriers
        .get_or_insert_with(|| Cache {
            module: module_key,
            entries: BTreeMap::new(),
        })
        .entries
        .insert(key, false);
    Ok(())
}
/// The ONLY production caller is the raw module-owned root loop, after the
/// complete original shape bound succeeds. Generic shape helpers and temporary
/// inferred Types cannot register a success or a lifetime-unstable address.
pub(crate) fn register_actual_carrier_root(
    module: &TypedModule,
    root: &Type,
    b: &mut Budget,
) -> Check<()> {
    b.spend(3)?; // root entry, queue header and final primitive-buffer release
    let mut pending = vec![root];
    while let Some(ty) = pending.pop() {
        b.spend(1)?;
        if element(ty).is_some_and(plain_element) {
            register(module, ty, b)?;
        }
        match ty {
            Type::Managed(inner)
            | Type::Desc(inner)
            | Type::OwnedStore { element: inner }
            | Type::Address { inner, .. }
            | Type::Array { element: inner, .. }
            | Type::BorrowedStorage { storage: inner, .. } => {
                b.spend(3)?;
                pending.push(inner);
            }
            Type::Tuple(fields) => {
                b.spend(fields.len().saturating_mul(3))?;
                pending.extend(fields);
            }
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                b.spend(fields.len().saturating_mul(3))?;
                pending.extend(fields.iter().map(|(_, field)| field));
            }
            Type::Enum { variants, .. } => {
                b.spend(variants.len())?;
                for (_, fields) in variants {
                    b.spend(fields.len().saturating_mul(3))?;
                    pending.extend(fields);
                }
            }
            Type::ErrorUnion { error, result } => {
                b.spend(6)?;
                pending.extend([error.as_ref(), result.as_ref()]);
            }
            // Uncollected wrappers deliberately remain cold. This inventory
            // performs no new semantic classification or rejection.
            _ => {}
        }
    }
    Ok(())
}
#[cfg(test)]
thread_local! { pub(super) static HITS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) }; }
pub(super) fn query(module: &TypedModule, ty: &Type, b: &mut Budget) -> Check<Option<bool>> {
    b.spend(1)?;
    let Some(cache) = &b.pod_carriers else {
        return Ok(None);
    };
    if cache.module != module as *const TypedModule as usize {
        return Ok(None);
    }
    let len = cache.entries.len();
    lookup(b, len)?;
    let result = b
        .pod_carriers
        .as_ref()
        .unwrap()
        .entries
        .get(&(ty as *const Type as usize))
        .copied();
    #[cfg(test)]
    if result == Some(true) {
        HITS.with(|hits| hits.set(hits.get() + 1));
    }
    Ok(result)
}
pub(super) fn complete(module: &TypedModule, ty: &Type, b: &mut Budget) -> Check<()> {
    // Cold admission is already complete. Charge the actual update search and
    // field write before changing a registered node; missing keys never insert.
    b.spend(1)?;
    let Some(cache) = &b.pod_carriers else {
        return Ok(());
    };
    if cache.module != module as *const TypedModule as usize {
        return Ok(());
    }
    let len = cache.entries.len();
    lookup(b, len)?;
    b.spend(1)?;
    if let Some(entry) = b
        .pod_carriers
        .as_mut()
        .unwrap()
        .entries
        .get_mut(&(ty as *const Type as usize))
    {
        *entry = true;
    }
    Ok(())
}
#[cfg(test)]
mod tests;
