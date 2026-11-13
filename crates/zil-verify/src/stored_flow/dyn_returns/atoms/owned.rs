//! A finite inline region comes from the actual input head's physical Type.
//! Address/Desc payloads terminate traversal; metadata paths never become GEPs.
use super::*;

pub(super) fn resolve(
    _module: &TypedModule,
    f: &TypedFunction,
    head: &StoragePosition,
    leaf: &Type,
    access: StoredBorrowMode,
    b: &mut Budget,
) -> Check<(Type, Option<StorageBinder>, Selection)> {
    let StorageBase::Parameter(parameter) = head.base else {
        return Err(bad("owned view has no actual input head"));
    };
    let Type::Address { mutable, inner } = parameter_type(f, parameter)? else {
        return Err(unsupported(
            "owned view requires an actual borrowed owner head",
            None,
        ));
    };
    if mode(leaf)? != access || (access == StoredBorrowMode::ExclusiveAddress && !mutable) {
        return Err(bad(
            "owned view access exceeds its complete leaf or actual head permission",
        ));
    }
    let Type::Address { inner: wanted, .. } = leaf else {
        return Err(bad("owned view does not select a complete address leaf"));
    };
    // No child class is inferred from equal local class numbers. Until a full
    // qualified occurrence map is available, only intrinsically class-free
    // concrete inline Types can be selected by this region consumer.
    if !plain(wanted, 0, b)? {
        return Err(unsupported(
            "owned region leaf requires its complete qualified occurrence map",
            None,
        ));
    }
    let prefix = path_places(&head.path, b)?;
    let root = crate::dynamic_ports::physical_path(inner, &prefix, b)?;
    let binder = relation(f, StorageBase::Parameter(parameter), &[], b)?;
    b.spend(prefix.len().saturating_add(2))?;
    let mut pending = vec![(root, prefix)];
    let mut sources = Vec::new();
    let mut visited = 0usize;
    while let Some((ty, path)) = pending.pop() {
        b.spend(path.len().saturating_add(1))?;
        visited = visited
            .checked_add(1)
            .ok_or_else(|| limit("owned region node count overflow"))?;
        if visited > b.limits.max_nodes {
            return Err(limit("owned region exceeds selected node bound"));
        }
        if path.len() > b.limits.max_call_depth.min(128) {
            return Err(limit("owned region exceeds selected physical depth"));
        }
        dynamic_metadata::ty(ty, b)?;
        dynamic_metadata::ty(wanted, b)?;
        if ty == wanted.as_ref() {
            b.spend(path.len().saturating_add(sources.len()).saturating_add(2))?;
            sources.push(Source {
                static_dependency: None,
                base: HoldRoot::Parameter(parameter),
                retained_port: None,
                leaf: Vec::new(),
                inline: path.clone(),
                heap_element: None,
                heap_parents: Vec::new(),
            });
        }
        match ty {
            Type::Tuple(fields) => {
                for (index, child) in fields.iter().enumerate() {
                    push(&mut pending, sources.len(), &path, index, child, b)?;
                }
            }
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                for (index, (_, child)) in fields.iter().enumerate() {
                    push(&mut pending, sources.len(), &path, index, child, b)?;
                }
            }
            Type::Array { element, length } => {
                if *length as usize > b.limits.max_nodes {
                    return Err(limit("owned region array exceeds selected node bound"));
                }
                for index in 0..*length as usize {
                    push(&mut pending, sources.len(), &path, index, element, b)?;
                }
            }
            // A borrowed payload is an external edge. Owning opaque/heap
            // edges require their actual allocation and retirement ledger.
            Type::Address { .. }
            | Type::Desc(_)
            | Type::Managed(_)
            | Type::OwnedDyn { .. }
            | Type::Dyn { .. }
            | Type::BorrowedStorage { .. }
            | Type::Closure(_)
            | Type::Task { .. }
            | Type::ErrorUnion { .. }
            | Type::Enum { .. } => {}
            _ => {}
        }
    }
    if sources.is_empty() {
        return Err(bad(
            "owned view selects no real inline region of the complete leaf Type",
        ));
    }
    dynamic_metadata::ty(leaf, b)?;
    Ok((leaf.clone(), Some(binder), Selection::Region(sources)))
}

fn push<'a>(
    pending: &mut Vec<(&'a Type, Path)>,
    rows: usize,
    parent: &[PlaceProjection],
    index: usize,
    child: &'a Type,
    b: &mut Budget,
) -> Check<()> {
    if pending.len().saturating_add(rows) >= b.limits.max_nodes {
        return Err(limit("owned region directory exceeds selected node bound"));
    }
    let index =
        u32::try_from(index).map_err(|_| limit("owned region field exceeds index domain"))?;
    b.spend(parent.len().saturating_add(2))?;
    let mut path = parent.to_vec();
    path.push(PlaceProjection::Field(index));
    pending.push((child, path));
    Ok(())
}

fn plain(ty: &Type, depth: usize, b: &mut Budget) -> Check<bool> {
    b.spend(1)?;
    if depth > b.limits.max_call_depth.min(128) {
        return Err(limit("owned leaf exceeds selected qualified depth"));
    }
    Ok(match ty {
        Type::Unit
        | Type::Bool
        | Type::I8
        | Type::I16
        | Type::I32
        | Type::I64
        | Type::Isize
        | Type::U8
        | Type::U16
        | Type::U32
        | Type::U64
        | Type::Usize
        | Type::F32
        | Type::F64 => true,
        Type::Tuple(fields) => {
            for child in fields {
                if !plain(child, depth + 1, b)? {
                    return Ok(false);
                }
            }
            true
        }
        Type::Array { element, .. } => plain(element, depth + 1, b)?,
        _ => false,
    })
}
