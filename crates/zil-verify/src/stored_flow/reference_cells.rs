//! Actual pointer-cell contents. Indirection is a private memory coordinate,
//! separate from physical fields, opaque ports and lifetime class metadata.
use super::*;
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Cell {
    pub(super) location: Source,
    pub(super) value: Type,
    pub(super) permission: Permission,
    pub(super) writable: bool,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Cells(pub(super) Vec<Cell>);
#[derive(Clone, Debug, PartialEq, Eq)]
struct Output {
    location: Source,
    value: Type,
    sources: Vec<Source>,
    parents: Vec<Source>,
    stable: bool,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct After(Vec<Output>);

/// The complete constructor domain; a nonzero ordinary aggregate requires its
/// existing qualified envelope. This count never chooses an outside image.
pub(crate) fn domain(module: Option<&TypedModule>, ty: &Type) -> Option<u32> {
    match ty {
        Type::Address { inner, .. } | Type::Desc(inner) => domain(module, inner)?.checked_add(1),
        Type::BorrowedStorage { life, .. } => Some(life.binder_count),
        Type::Managed(inner) => domain(module, inner),
        Type::Closure(id) => module?
            .closures
            .get(id.0 as usize)?
            .storage_life
            .as_ref()
            .map(|h| h.fixed_binder_count),
        Type::Task { retained_life, .. } | Type::MutexGuard { retained_life, .. } => {
            Some(retained_life.binder_count)
        }
        Type::Dyn { interface } | Type::OwnedDyn { interface } => module?
            .dyn_interfaces
            .iter()
            .find(|row| row.id == *interface)
            .map(|row| row.retained_life.binder_count),
        Type::Tuple(fields) => fields
            .iter()
            .all(|ty| domain(module, ty) == Some(0))
            .then_some(0),
        Type::Record { fields, .. } | Type::Package { fields, .. } => fields
            .iter()
            .all(|(_, ty)| domain(module, ty) == Some(0))
            .then_some(0),
        Type::Enum { variants, .. } => variants
            .iter()
            .all(|(_, fields)| fields.iter().all(|ty| domain(module, ty) == Some(0)))
            .then_some(0),
        Type::ErrorUnion { error, result } => {
            (domain(module, error) == Some(0) && domain(module, result) == Some(0)).then_some(0)
        }
        Type::Array { element, .. } | Type::Slice { element } | Type::OwnedStore { element } => {
            (domain(module, element) == Some(0)).then_some(0)
        }
        Type::Unit | Type::Bool | Type::ByteBuffer | Type::ForeignHandle | Type::CWideString => {
            Some(0)
        }
        ty if ty.is_integer() || ty.is_float() => Some(0),
        _ => None,
    }
}
pub(super) fn nested(ty: &Type) -> bool {
    matches!(ty,Type::Address{inner,..}if matches!(inner.as_ref(),Type::Address{..}))
}
fn index(rows: &Cells, key: &Source, b: &mut Budget) -> Check<Option<usize>> {
    b.spend(rows.0.len())?;
    for (i, row) in rows.0.iter().enumerate() {
        if same_target(&row.location, key, b)? {
            return Ok(Some(i));
        }
    }
    Ok(None)
}
pub(super) fn work(rows: &Cells, b: &mut Budget) -> Check<()> {
    b.spend(rows.0.len())?;
    for row in &rows.0 {
        source_work(std::slice::from_ref(&row.location), b)?;
        dynamic_metadata::ty(&row.value, b)?;
        permission_work(&row.permission, b)?;
    }
    Ok(())
}
pub(super) fn after_work(rows: &After, b: &mut Budget) -> Check<()> {
    b.spend(rows.0.len())?;
    for row in &rows.0 {
        source_work(std::slice::from_ref(&row.location), b)?;
        dynamic_metadata::ty(&row.value, b)?;
        source_work(&row.sources, b)?;
        source_work(&row.parents, b)?;
    }
    Ok(())
}
fn insert(rows: &mut Cells, row: &Cell, b: &mut Budget) -> Check<()> {
    if let Some(at) = index(rows, &row.location, b)? {
        dynamic_metadata::ty(&rows.0[at].value, b)?;
        dynamic_metadata::ty(&row.value, b)?;
        if rows.0[at].value != row.value || rows.0[at].writable != row.writable {
            return Err(err(
                "pointer cell changes its complete stored type or access role",
            ));
        }
        join_permission(&mut rows.0[at].permission, &row.permission, b)?;
    } else {
        source_work(std::slice::from_ref(&row.location), b)?;
        dynamic_metadata::ty(&row.value, b)?;
        permission_work(&row.permission, b)?;
        b.spend(1)?;
        rows.0.push(row.clone());
    }
    Ok(())
}
pub(super) fn join(to: &mut Cells, from: &Cells, b: &mut Budget) -> Check<()> {
    for row in &from.0 {
        insert(to, row, b)?;
    }
    Ok(())
}
fn next(source: &Source, b: &mut Budget) -> Check<Source> {
    if source.heap_element.is_some() || !source.heap_parents.is_empty() {
        return Err(err("raw pointer cell cannot infer a heap allocation route"));
    }
    source_work(std::slice::from_ref(source), b)?;
    let mut target = source.clone();
    let mut steps = match &source.retained_port {
        None => vec![],
        Some(PortOrigin::Reference { steps }) => {
            b.spend(
                steps
                    .iter()
                    .fold(steps.len(), |n, path| n.saturating_add(path.len())),
            )?;
            steps.clone()
        }
        _ => return Err(err("opaque port is not a raw pointer-cell address")),
    };
    if steps.len() >= b.limits.max_call_depth {
        return Err(limit("pointer-cell indirection exceeds selected depth"));
    }
    b.spend(source.inline.len().saturating_add(1))?;
    steps.push(std::mem::take(&mut target.inline));
    target.retained_port = Some(PortOrigin::Reference { steps });
    Ok(target)
}
fn legacy<'a>(state: &'a Frame, source: &Source, b: &mut Budget) -> Check<Option<&'a Permission>> {
    if source.retained_port.is_some()
        || source.heap_element.is_some()
        || !source.heap_parents.is_empty()
    {
        return Ok(None);
    }
    let payload = match source.base {
        HoldRoot::Local(id) => state.locals.get(id.0 as usize),
        HoldRoot::Parameter(i) => state.external.get(i as usize),
        HoldRoot::Capture(i) => state.captures.get(i as usize),
        HoldRoot::Static => None,
    };
    let Some(payload) = payload else {
        return Ok(None);
    };
    if matches!(source.base, HoldRoot::Local(_))
        && source.leaf.is_empty()
        && source.inline.is_empty()
    {
        return Ok(Some(&payload.address));
    }
    // A nonempty leaf denotes the saved pointer's target, not its cell.
    if source.leaf.is_empty() && !source.inline.is_empty() {
        return leaf(payload, &source.inline, b);
    }
    Ok(None)
}
fn lookup(state: &Frame, source: &Source, b: &mut Budget) -> Check<Permission> {
    if let Some(permission) = legacy(state, source, b)? {
        permission_work(permission, b)?;
        return Ok(permission.clone());
    }
    if let Some(at) = index(&state.reference_cells, source, b)? {
        let permission = &state.reference_cells.0[at].permission;
        permission_work(permission, b)?;
        return Ok(permission.clone());
    }
    Ok(Permission::default())
}
pub(super) fn read(state: &Frame, pointer: &Permission, b: &mut Budget) -> Check<Permission> {
    permission_work(pointer, b)?;
    let mut value = Permission::default();
    for source in &pointer.sources {
        let current = lookup(state, source, b)?;
        if current.sources.is_empty() {
            return Ok(Permission::default());
        }
        join_permission(&mut value, &current, b)?;
    }
    Ok(value)
}
fn seed_chain(
    module: &TypedModule,
    ty: &Type,
    pointer: &Source,
    state: &mut Frame,
    loans: &mut Vec<Loan>,
    depth: usize,
    b: &mut Budget,
) -> Check<()> {
    if depth > b.limits.max_call_depth.min(128) {
        return Err(limit("pointer-cell input graph exceeds selected depth"));
    }
    let Type::Address { mutable, inner } = ty else {
        return Ok(());
    };
    if matches!(inner.as_ref(), Type::Address { .. }) {
        let target = next(pointer, b)?;
        let id = loans.len();
        source_work(std::slice::from_ref(&target), b)?;
        source_work(std::slice::from_ref(&target), b)?;
        source_work(std::slice::from_ref(pointer), b)?;
        dynamic_metadata::ty(inner, b)?;
        b.spend(4)?;
        let exclusive = matches!(inner.as_ref(), Type::Address { mutable: true, .. });
        loans.push(Loan {
            receiver_head: false,
            sources: vec![target.clone()],
            parents: vec![],
            exclusive,
        });
        b.stored_exclusive_loans |= exclusive;
        let permission = Permission {
            sources: vec![target.clone()],
            loans: vec![id],
        };
        insert(
            &mut state.reference_cells,
            &Cell {
                location: pointer.clone(),
                value: *inner.clone(),
                permission,
                writable: *mutable,
            },
            b,
        )?;
        seed_chain(module, inner, &target, state, loans, depth + 1, b)?;
    } else if let Type::BorrowedStorage { storage, life } = inner.as_ref() {
        let mut needed = false;
        for slot in &life.slots {
            needed |= nested(crate::dynamic_ports::physical_path(storage, &slot.path, b)?);
        }
        if !needed {
            return Ok(());
        }
        for slot in &life.slots {
            let ty = crate::dynamic_ports::physical_path(storage, &slot.path, b)?;
            if !nested(ty) {
                continue;
            }
            source_work(std::slice::from_ref(pointer), b)?;
            let mut at = pointer.clone();
            b.spend(slot.path.len())?;
            at.inline.extend(&slot.path);
            let permission = if let Some(existing) = legacy(state, &at, b)? {
                permission_work(existing, b)?;
                existing.clone()
            } else {
                let target = next(&at, b)?;
                let id = loans.len();
                source_work(std::slice::from_ref(&target), b)?;
                b.spend(3)?;
                let exclusive = matches!(ty, Type::Address { mutable: true, .. });
                loans.push(Loan {
                    receiver_head: false,
                    sources: vec![target.clone()],
                    parents: vec![],
                    exclusive,
                });
                b.stored_exclusive_loans |= exclusive;
                Permission {
                    sources: vec![target],
                    loans: vec![id],
                }
            };
            if legacy(state, &at, b)?.is_none() {
                dynamic_metadata::ty(ty, b)?;
                permission_work(&permission, b)?;
                insert(
                    &mut state.reference_cells,
                    &Cell {
                        location: at,
                        value: ty.clone(),
                        permission: permission.clone(),
                        writable: *mutable,
                    },
                    b,
                )?;
            }
            for target in &permission.sources {
                seed_chain(module, ty, target, state, loans, depth + 1, b)?;
            }
        }
    }
    Ok(())
}
pub(super) fn seed(
    module: &TypedModule,
    f: &TypedFunction,
    state: &mut Frame,
    loans: &mut Vec<Loan>,
    b: &mut Budget,
) -> Check<()> {
    if !b.raw_reference_cells {
        return Ok(());
    }
    b.spend(f.parameters.len())?;
    for (index, local) in f.parameters.iter().enumerate() {
        let ty = &f.locals[local.0 as usize].ty;
        if !matches!(ty, Type::Address { .. }) {
            continue;
        }
        let source = Source {
            static_dependency: StaticProvenance::NotStatic,
            retained_port: None,
            base: HoldRoot::Parameter(index as u32),
            leaf: vec![],
            inline: vec![],
            heap_parents: vec![],
            heap_element: None,
        };
        seed_chain(module, ty, &source, state, loans, 0, b)?;
    }
    if let Some(definition) = captured_definition(module, f, b) {
        for (index, ty) in definition.captures.iter().enumerate() {
            if !matches!(ty, Type::Address { .. }) {
                continue;
            }
            let source = Source {
                static_dependency: StaticProvenance::NotStatic,
                retained_port: None,
                base: HoldRoot::Capture(index as u32),
                leaf: vec![],
                inline: vec![],
                heap_parents: vec![],
                heap_element: None,
            };
            seed_chain(module, ty, &source, state, loans, 0, b)?;
        }
    }
    Ok(())
}
pub(super) fn substitute(
    module: &TypedModule,
    f: &TypedFunction,
    source: &Source,
    args: &[ValueId],
    values: &[Payload],
    state: &Frame,
    b: &mut Budget,
) -> Check<Permission> {
    if !matches!(source.retained_port, Some(PortOrigin::Reference { .. })) {
        return Err(err(
            "pointer-cell substitution lost its real indirection directory",
        ));
    }
    source_work(std::slice::from_ref(source), b)?;
    let mut head = source.clone();
    head.retained_port = None;
    head.inline.clear();
    let permission = instantiate(module, f, &head, args, values, state, b)?;
    follow(source, permission, state, b)
}
pub(super) fn follow(
    source: &Source,
    mut permission: Permission,
    state: &Frame,
    b: &mut Budget,
) -> Check<Permission> {
    let Some(PortOrigin::Reference { steps }) = &source.retained_port else {
        return Err(err(
            "raw reference transport lacks its exact cell directory",
        ));
    };
    for path in steps {
        for target in &mut permission.sources {
            b.spend(path.len())?;
            target.inline.extend(path);
        }
        permission = read(state, &permission, b)?;
    }
    for target in &mut permission.sources {
        store_paths::compose(target, source, b)?;
    }
    Ok(permission)
}
/// Merely holding an outer pointer also preserves the loans currently stored
/// in its reachable cells. Loading a shared inner pointer does not parent its
/// result to the short outer slot loan.
pub(super) fn held(state: &Frame, pointer: &Permission, b: &mut Budget) -> Check<Vec<usize>> {
    source_work(&pointer.sources, b)?;
    let mut pending = pointer.sources.clone();
    let mut seen = Vec::new();
    let mut result = Vec::new();
    while let Some(source) = pending.pop() {
        b.spend(seen.len().saturating_add(1))?;
        if seen.contains(&source) {
            continue;
        }
        source_work(std::slice::from_ref(&source), b)?;
        seen.push(source.clone());
        if seen.len() > b.limits.max_nodes {
            return Err(limit("pointer-cell reachable loan directory exceeded"));
        }
        let value = lookup(state, &source, b)?;
        add_loans(&mut result, &value.loans, b)?;
        source_work(&value.sources, b)?;
        pending.extend(value.sources);
    }
    Ok(result)
}
fn class_map(
    life: &StorageLifeShape,
    map: &[u32],
    path: &[LifeTypeProjection],
    count: u32,
    b: &mut Budget,
) -> Check<Vec<u32>> {
    b.spend(count as usize)?;
    let mut out = Vec::new();
    for class in 0..count {
        b.spend(
            life.retained
                .len()
                .saturating_mul(path.len().saturating_add(1)),
        )?;
        let mut rows = life
            .retained
            .iter()
            .filter(|r| r.path == path && r.class == class);
        let row = rows
            .next()
            .ok_or_else(|| err("pointer-cell projection omits a complete child class"))?;
        if rows.next().is_some() {
            return Err(err("pointer-cell projection repeats a whole child class"));
        }
        out.push(
            *map.get(row.binder as usize)
                .ok_or_else(|| err("pointer-cell child map escapes its full domain"))?,
        );
    }
    Ok(out)
}
fn projected<'a>(
    module: &TypedModule,
    mut ty: &'a Type,
    mut life: StorageLifeShape,
    mut map: Vec<u32>,
    path: &[PlaceProjection],
    b: &mut Budget,
) -> Check<(&'a Type, StorageLifeShape, Vec<u32>)> {
    for step in path {
        while let Type::BorrowedStorage { storage, .. } = ty {
            ty = storage;
        }
        let child = crate::dynamic_ports::physical_path(ty, &[*step], b)?;
        let child_life = crate::tasks::frame_shape::declared_life(module, child, b)?;
        let prefix = [LifeTypeProjection::Place(*step)];
        let child_map = if matches!(child, Type::Address { .. }) {
            b.spend(child_life.binder_count as usize)?;
            let mut next = vec![u32::MAX; child_life.binder_count as usize];
            b.spend(life.slots.len())?;
            let mut slots = life.slots.iter().filter(|row| row.path == [*step]);
            let slot = slots
                .next()
                .ok_or_else(|| err("pointer-cell field has no exact actual borrowed slot"))?;
            if slots.next().is_some()
                || slot.mode
                    != match child {
                        Type::Address { mutable: true, .. } => StoredBorrowMode::ExclusiveAddress,
                        _ => StoredBorrowMode::SharedAddress,
                    }
            {
                return Err(err(
                    "pointer-cell field changes exact physical slot mode or uniqueness",
                ));
            }
            if next.is_empty() {
                return Err(err("pointer-cell field loses its variable outer domain"));
            }
            next[0] = *map
                .get(slot.binder as usize)
                .ok_or_else(|| err("pointer-cell field outer map is incomplete"))?;
            for row in &child_life.retained {
                b.spend(prefix.len().saturating_add(row.path.len()))?;
                let mut at = prefix.to_vec();
                at.extend(&row.path);
                let class = class_map(&life, &map, &at, row.class + 1, b)?[row.class as usize];
                *next
                    .get_mut(row.binder as usize)
                    .ok_or_else(|| err("pointer-cell field inner domain is incomplete"))? = class;
            }
            if next.contains(&u32::MAX) {
                return Err(err("pointer-cell field discards a complete inner class"));
            }
            next
        } else {
            class_map(&life, &map, &prefix, child_life.binder_count, b)?
        };
        ty = child;
        life = child_life;
        map = child_map;
    }
    Ok((ty, life, map))
}
pub(super) fn relation(
    module: &TypedModule,
    f: &TypedFunction,
    source: &Source,
    b: &mut Budget,
) -> Check<StorageBinder> {
    let (base, ty) = match source.base {
        HoldRoot::Parameter(index) => (
            StorageBase::Parameter(index),
            &f.locals[f
                .parameters
                .get(index as usize)
                .ok_or_else(|| err("pointer-cell source parameter is absent"))?
                .0 as usize]
                .ty,
        ),
        HoldRoot::Capture(index) => (
            StorageBase::Capture(index),
            captured_definition(module, f, b)
                .and_then(|d| d.captures.get(index as usize))
                .ok_or_else(|| err("pointer-cell capture has no exact environment occurrence"))?,
        ),
        _ => return Err(err("pointer-cell source has no original input relation")),
    };
    let full = crate::tasks::frame_shape::life(module, ty, f, base, b, 0)?;
    dynamic_metadata::life_shape(&full, b)?;
    let mut life = full.clone();
    b.spend(full.binder_count as usize)?;
    let mut map = (0..full.binder_count).collect::<Vec<_>>();
    let mut current = ty;
    if !source.leaf.is_empty() {
        if let Type::Address { inner, .. } = current {
            let child = crate::tasks::frame_shape::declared_life(module, inner, b)?;
            map = class_map(
                &life,
                &map,
                &[LifeTypeProjection::Pointee],
                child.binder_count,
                b,
            )?;
            life = child;
            current = inner;
        }
        (current, life, map) = projected(module, current, life, map, &source.leaf, b)?;
    }
    if let Some(PortOrigin::Reference { steps }) = &source.retained_port {
        for path in steps {
            let Type::Address { inner, .. } = current else {
                return Err(err("pointer-cell source crosses a nonpointer type"));
            };
            let child = crate::tasks::frame_shape::declared_life(module, inner, b)?;
            map = class_map(
                &life,
                &map,
                &[LifeTypeProjection::Pointee],
                child.binder_count,
                b,
            )?;
            life = child;
            current = inner;
            (current, life, map) = projected(module, current, life, map, path, b)?;
        }
    }
    let slot = life
        .slots
        .iter()
        .find(|row| row.path.is_empty())
        .ok_or_else(|| err("pointer-cell source loses its complete pointer outer occurrence"))?;
    let class = *map
        .get(slot.binder as usize)
        .ok_or_else(|| err("pointer-cell source loses its complete outer class map"))?;
    crate::tasks::frame_shape::image(f, base, &full, class, b)
}
fn put(
    state: &mut Frame,
    location: &Source,
    value: &Type,
    permission: &Permission,
    strong: bool,
    b: &mut Budget,
) -> Check<()> {
    if location.base == HoldRoot::Static {
        return Err(err("pointer-cell write targets immutable static storage"));
    }
    if let HoldRoot::Local(id) = location.base
        && location.retained_port.is_none()
    {
        let payload = state
            .locals
            .get_mut(id.0 as usize)
            .ok_or_else(|| err("pointer-cell local is absent"))?;
        if location.leaf.is_empty() && location.inline.is_empty() {
            if strong {
                permission_work(permission, b)?;
                payload.address = permission.clone();
            } else {
                join_permission(&mut payload.address, permission, b)?;
            }
            return Ok(());
        }
    }
    if let Some(at) = index(&state.reference_cells, location, b)? {
        dynamic_metadata::ty(&state.reference_cells.0[at].value, b)?;
        dynamic_metadata::ty(value, b)?;
        if state.reference_cells.0[at].value != *value {
            return Err(err(
                "pointer-cell write changes its complete declared value",
            ));
        }
        if strong {
            permission_work(permission, b)?;
            state.reference_cells.0[at].permission = permission.clone();
        } else {
            join_permission(&mut state.reference_cells.0[at].permission, permission, b)?;
        }
    } else if legacy(state, location, b)?.is_none() {
        return Err(err(
            "pointer-cell write has no independently initialized current cell",
        ));
    }
    if location.retained_port.is_none() && location.leaf.is_empty() && !location.inline.is_empty() {
        let payload = match location.base {
            HoldRoot::Local(id) => state.locals.get_mut(id.0 as usize),
            HoldRoot::Parameter(index) => state.external.get_mut(index as usize),
            HoldRoot::Capture(index) => state.captures.get_mut(index as usize),
            _ => None,
        }
        .ok_or_else(|| err("pointer-cell physical holder is absent"))?;
        if strong {
            payload
                .leaves
                .retain(|(path, _)| *path != location.inline, b)?;
        }
        add_leaf(payload, &location.inline, permission, b)?;
    }
    Ok(())
}
fn retain(
    permission: &Permission,
    instruction: &Instruction,
    location: &Source,
    sites: &mut dyn_leases::Sites,
    loans: &mut Vec<Loan>,
    b: &mut Budget,
) -> Check<Permission> {
    source_work(std::slice::from_ref(location), b)?;
    let mut out = Permission::default();
    for source in &permission.sources {
        source_work(std::slice::from_ref(source), b)?;
        b.spend(permission.loans.len().saturating_add(1))?;
        let input = Permission {
            sources: vec![source.clone()],
            loans: permission.loans.clone(),
        };
        let held = dyn_leases::retain(
            sites,
            loans,
            instruction,
            dyn_leases::Destination::Reference(location.clone()),
            dyn_leases::Component::Summary(source.clone()),
            StoredBorrowMode::SharedAddress,
            &input,
            b,
        )?;
        join_permission(&mut out, &held, b)?;
    }
    Ok(out)
}
pub(super) fn write(
    module: &TypedModule,
    f: &TypedFunction,
    instruction: &Instruction,
    state: &mut Frame,
    address: &Permission,
    value: &Permission,
    value_type: &Type,
    sites: &mut dyn_leases::Sites,
    loans: &mut Vec<Loan>,
    b: &mut Budget,
) -> Check<()> {
    permission_work(address, b)?;
    permission_work(value, b)?;
    if address.sources.is_empty() || value.sources.is_empty() {
        state.pending_external_update = true;
        return Ok(());
    }
    let strong = address.sources.len() == 1;
    for location in &address.sources {
        if matches!(location.base, HoldRoot::Parameter(_) | HoldRoot::Capture(_)) {
            let expected = relation(module, f, &next(location, b)?, b)?;
            for source in &value.sources {
                if source.base == HoldRoot::Static {
                    if source.static_dependency == StaticProvenance::NotStatic {
                        return Err(err(
                            "pointer-cell static replacement has no actual provenance",
                        ));
                    }
                    continue;
                }
                if relation(module, f, source, b)? != expected {
                    return Err(err(
                        "pointer-cell replacement exceeds the complete declared inner lifetime",
                    ));
                }
            }
        }
        let value = retain(value, instruction, location, sites, loans, b)?;
        put(state, location, value_type, &value, strong, b)?;
    }
    Ok(())
}
pub(super) fn summarize(
    f: &TypedFunction,
    entry: &Frame,
    state: &Frame,
    loans: &[Loan],
    summary: &mut Summary,
    b: &mut Budget,
) -> Check<()> {
    b.spend(state.reference_cells.0.len())?;
    for row in &state.reference_cells.0 {
        if !row.writable
            || !matches!(
                row.location.base,
                HoldRoot::Parameter(_) | HoldRoot::Capture(_)
            )
        {
            continue;
        }
        let current = lookup(state, &row.location, b)?;
        if current.sources.is_empty() {
            summary.complete = false;
            continue;
        }
        let before = lookup(entry, &row.location, b)?;
        permission_work(&before, b)?;
        permission_work(&current, b)?;
        let stable = current == before;
        let parents = dyn_returns::parents(&current, loans, b)?;
        for source in current.sources.iter().chain(&parents) {
            if !matches!(
                source.base,
                HoldRoot::Parameter(_) | HoldRoot::Capture(_) | HoldRoot::Static
            ) {
                return Err(err(
                    "pointer-cell after-state retains activation-local storage",
                ));
            }
        }
        b.spend(
            summary
                .reference_after
                .0
                .len()
                .saturating_mul(width(&row.location).saturating_add(1)),
        )?;
        if let Some(old) = summary
            .reference_after
            .0
            .iter_mut()
            .find(|old| old.location == row.location)
        {
            old.stable &= stable;
            add_sources(&mut old.sources, &current.sources, b)?;
            add_sources(&mut old.parents, &parents, b)?;
        } else {
            source_work(std::slice::from_ref(&row.location), b)?;
            dynamic_metadata::ty(&row.value, b)?;
            source_work(&current.sources, b)?;
            b.spend(1)?;
            summary.reference_after.0.push(Output {
                location: row.location.clone(),
                value: row.value.clone(),
                sources: current.sources,
                parents,
                stable,
            });
        }
    }
    let _ = f;
    Ok(())
}
pub(super) struct Prepared {
    updates: Vec<(Permission, Type, Permission)>,
    pending: bool,
    weak: bool,
}
pub(super) fn prepare(
    module: &TypedModule,
    f: &TypedFunction,
    instruction: &Instruction,
    summary: &Summary,
    args: &[ValueId],
    definition: Option<&ClosureDefinition>,
    values: &[Payload],
    state: &Frame,
    sites: &mut dyn_leases::Sites,
    loans: &mut Vec<Loan>,
    b: &mut Budget,
) -> Check<Prepared> {
    let mut updates = Vec::new();
    let mut pending = !summary.complete;
    b.spend(summary.reference_after.0.len())?;
    for row in &summary.reference_after.0 {
        let resolve = |source: &Source, b: &mut Budget| {
            if let Some(d) = definition {
                dyn_captures::substitute(module, f, instruction, source, d, args, values, state, b)
            } else {
                instantiate(module, f, source, args, values, state, b)
            }
        };
        let address = resolve(&row.location, b)?;
        let mut permission = Permission::default();
        for source in &row.sources {
            join_permission(&mut permission, &resolve(source, b)?, b)?;
        }
        if permission.sources.is_empty() || address.sources.is_empty() {
            pending = true;
            continue;
        }
        for parent in &row.parents {
            let actual = resolve(parent, b)?;
            if actual.sources.is_empty() {
                pending = true;
            }
            add_loans(&mut permission.loans, &actual.loans, b)?;
        }
        if !row.stable {
            permission = retain(&permission, instruction, &row.location, sites, loans, b)?;
        }
        b.spend(1)?;
        dynamic_metadata::ty(&row.value, b)?;
        updates.push((address, row.value.clone(), permission));
    }
    Ok(Prepared {
        updates,
        pending,
        weak: false,
    })
}
pub(super) fn publish(state: &mut Frame, prepared: Prepared, b: &mut Budget) -> Check<()> {
    state.pending_external_update |= prepared.pending;
    for (address, ty, permission) in prepared.updates {
        let strong = !prepared.weak && address.sources.len() == 1;
        for location in &address.sources {
            put(state, location, &ty, &permission, strong, b)?;
        }
    }
    Ok(())
}

/// The loan identities exported to the ordinary borrow pass are the real
/// incoming content loans, never the loans for the containing pointer slots.
pub(super) fn inputs(solution: &Solution, b: &mut Budget) -> Check<Vec<(Source, bool)>> {
    let mut out = Vec::new();
    b.spend(solution.entry.reference_cells.0.len())?;
    for cell in &solution.entry.reference_cells.0 {
        for source in &cell.permission.sources {
            if !matches!(source.retained_port, Some(PortOrigin::Reference { .. })) {
                continue;
            }
            if input_index_optional(&out, source, b)?.is_some() {
                continue;
            }
            b.spend(cell.permission.loans.len())?;
            let mut exclusive = None;
            for id in &cell.permission.loans {
                let loan = solution
                    .loans
                    .get(*id)
                    .ok_or_else(|| err("incoming pointer contents lost their actual input loan"))?;
                for target in &loan.sources {
                    if same_target(source, target, b)? {
                        if exclusive.is_some_and(|mode| mode != loan.exclusive) {
                            return Err(err("incoming pointer content changes access mode"));
                        }
                        exclusive = Some(loan.exclusive);
                    }
                }
            }
            let exclusive = exclusive
                .ok_or_else(|| err("incoming pointer contents have no exact input loan"))?;
            source_work(std::slice::from_ref(source), b)?;
            b.spend(1)?;
            let mut source = source.clone();
            source.inline.clear();
            out.push((source, exclusive));
        }
    }
    Ok(out)
}
fn input_index_optional(
    inputs: &[(Source, bool)],
    source: &Source,
    b: &mut Budget,
) -> Check<Option<u32>> {
    for (index, (input, _)) in inputs.iter().enumerate() {
        b.spend(width(input).saturating_add(width(source)))?;
        if input.base == source.base
            && input.leaf == source.leaf
            && input.retained_port == source.retained_port
            && input.heap_element == source.heap_element
            && input.heap_parents == source.heap_parents
        {
            return Ok(Some(
                u32::try_from(index)
                    .map_err(|_| limit("pointer-content input namespace exceeded"))?,
            ));
        }
    }
    Ok(None)
}
pub(super) fn input_index(
    inputs: &[(Source, bool)],
    source: &Source,
    b: &mut Budget,
) -> Check<Option<u32>> {
    if !matches!(source.retained_port, Some(PortOrigin::Reference { .. })) {
        return Ok(None);
    }
    input_index_optional(inputs, source, b)?
        .map(Some)
        .ok_or_else(|| err("actual pointer-content source lacks its complete incoming occurrence"))
}

pub(super) fn prepare_call(
    module: &TypedModule,
    f: &TypedFunction,
    instruction: &Instruction,
    summaries: &[Summary],
    values: &[Payload],
    state: &Frame,
    sites: &mut dyn_leases::Sites,
    loans: &mut Vec<Loan>,
    b: &mut Budget,
) -> Check<Prepared> {
    if !b.raw_reference_cells {
        return Ok(Prepared {
            updates: Vec::new(),
            pending: false,
            weak: false,
        });
    }
    if let InstructionKind::Call { function, arguments }
    | InstructionKind::SuspendCall { function, arguments } = &instruction.kind
    {
        return prepare(
            module,
            f,
            instruction,
            &summaries[function.0 as usize],
            arguments,
            None,
            values,
            state,
            sites,
            loans,
            b,
        );
    }
    if let Some((definition, arguments)) = stored_callables::called(module, f, &instruction.kind) {
        return prepare(
            module,
            f,
            instruction,
            &summaries[definition.function.0 as usize],
            arguments,
            Some(definition),
            values,
            state,
            sites,
            loans,
            b,
        );
    }
    if let InstructionKind::CallDynSlot { arguments, .. } = &instruction.kind {
        b.spend(arguments.len())?;
        for argument in arguments {
            if matches!(vt(f,*argument)?,Type::Address{mutable:true,inner}if matches!(inner.as_ref(),Type::Address{..}))
            {
                return Err(unsupported(
                    "raw pointer-cell dynamic update requires an independently checked complete after contract",
                    Some(instruction.span),
                ));
            }
        }
    }
    if let InstructionKind::CallIndirect { callee, arguments } = &instruction.kind {
        b.spend(arguments.len())?;
        let mut writes = false;
        for argument in arguments {
            writes |= matches!(vt(f,*argument)?,Type::Address{mutable:true,inner}if matches!(inner.as_ref(),Type::Address{..}));
        }
        if writes {
            let targets = code_targets(f, *callee, b)?;
            let mut combined = Prepared {
                updates: Vec::new(),
                pending: false,
                weak: targets.len() != 1,
            };
            for target in targets {
                let summary = summaries
                    .get(target.0 as usize)
                    .ok_or_else(|| err("raw indirect call has no exact target summary"))?;
                let prepared = prepare(
                    module,
                    f,
                    instruction,
                    summary,
                    arguments,
                    None,
                    values,
                    state,
                    sites,
                    loans,
                    b,
                )?;
                b.spend(prepared.updates.len())?;
                combined.updates.extend(prepared.updates);
                combined.pending |= prepared.pending;
            }
            return Ok(combined);
        }
    }
    Ok(Prepared {
        updates: Vec::new(),
        pending: false,
        weak: false,
    })
}

/// Actual code provenance, conservatively including every Store alternative.
/// Unknown input code never selects a same-signature function as a substitute.
pub(super) fn code_targets(
    f: &TypedFunction,
    callee: ValueId,
    b: &mut Budget,
) -> Check<Vec<FunctionId>> {
    b.spend(1)?;
    let mut pending = vec![callee];
    let mut seen = Vec::new();
    let mut targets = Vec::new();
    while let Some(value) = pending.pop() {
        b.spend(seen.len().saturating_add(1))?;
        if seen.contains(&value) {
            continue;
        }
        seen.push(value);
        let mut producer = None;
        for block in &f.blocks {
            b.spend(block.instructions.len())?;
            for instruction in &block.instructions {
                if instruction.result == Some(value) {
                    if producer.replace(&instruction.kind).is_some() {
                        return Err(err("raw indirect call code has duplicate SSA producers"));
                    }
                }
            }
        }
        match producer {
            Some(InstructionKind::FunctionRef { function }) => {
                b.spend(targets.len().saturating_add(1))?;
                if !targets.contains(function) {
                    targets.push(*function);
                }
            }
            Some(InstructionKind::Load(local) | InstructionKind::TakeLocal { local }) => {
                b.spend(f.parameters.len())?;
                if f.parameters.contains(local) {
                    return Err(unsupported(
                        "raw pointer-cell indirect update needs complete actual code provenance",
                        Some(f.span),
                    ));
                }
                let mut stored = false;
                for block in &f.blocks {
                    b.spend(block.instructions.len())?;
                    for instruction in &block.instructions {
                        if let InstructionKind::Store {
                            local: target,
                            value,
                        } = &instruction.kind
                            && target == local
                        {
                            b.spend(1)?;
                            pending.push(*value);
                            stored = true;
                        }
                    }
                }
                if !stored {
                    return Err(err("raw indirect code local has no initialized producer"));
                }
            }
            _ => {
                return Err(unsupported(
                    "raw pointer-cell indirect update needs complete actual code provenance",
                    Some(f.span),
                ));
            }
        }
        if seen.len() > b.limits.max_nodes {
            return Err(limit(
                "raw indirect code provenance exceeded selected nodes",
            ));
        }
    }
    if targets.is_empty() {
        return Err(err("raw indirect call has no grounded executable target"));
    }
    Ok(targets)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn source(base: HoldRoot) -> Source {
        Source {
            base,
            retained_port: None,
            leaf: vec![],
            inline: vec![],
            heap_parents: vec![],
            heap_element: None,
            static_dependency: StaticProvenance::NotStatic,
        }
    }
    fn permission(base: HoldRoot, loan: usize) -> Permission {
        Permission {
            sources: vec![source(base)],
            loans: vec![loan],
        }
    }
    fn pointer() -> Type {
        Type::Address {
            mutable: false,
            inner: Box::new(Type::I32),
        }
    }
    #[test]
    fn shared_inner_copy_keeps_data_loan_without_short_cell_parent() {
        let mut state = Frame::default();
        state.locals = vec![Payload::default()];
        state.locals[0].address = permission(HoldRoot::Parameter(0), 3);
        let slot = permission(HoldRoot::Local(LocalId(0)), 9);
        let mut b = Budget::new(&Limits::default());
        let loaded = read(&state, &slot, &mut b).unwrap();
        assert_eq!(loaded, state.locals[0].address);
        assert!(!loaded.loans.contains(&9));
        assert_eq!(held(&state, &slot, &mut b).unwrap(), vec![3]);
    }
    #[test]
    fn may_target_replacement_preserves_unselected_cells_and_old_copies() {
        let mut state = Frame::default();
        state.locals = vec![Payload::default(); 2];
        state.locals[0].address = permission(HoldRoot::Parameter(0), 0);
        state.locals[1].address = permission(HoldRoot::Parameter(1), 1);
        let original = state.locals[0].address.clone();
        let replacement = permission(HoldRoot::Parameter(2), 2);
        let mut b = Budget::new(&Limits::default());
        for local in 0..2 {
            put(
                &mut state,
                &source(HoldRoot::Local(LocalId(local))),
                &pointer(),
                &replacement,
                false,
                &mut b,
            )
            .unwrap();
        }
        assert_eq!(original.sources, vec![source(HoldRoot::Parameter(0))]);
        for local in 0..2 {
            assert!(
                state.locals[local]
                    .address
                    .sources
                    .contains(&source(HoldRoot::Parameter(local as u32)))
            );
            assert!(
                state.locals[local]
                    .address
                    .sources
                    .contains(&source(HoldRoot::Parameter(2)))
            );
        }
        put(
            &mut state,
            &source(HoldRoot::Local(LocalId(0))),
            &pointer(),
            &replacement,
            true,
            &mut b,
        )
        .unwrap();
        assert_eq!(state.locals[0].address, replacement);
        assert_eq!(original.loans, vec![0]);
    }
    #[test]
    fn missing_possible_cell_never_becomes_partial_known_read() {
        let mut state = Frame::default();
        state.locals = vec![Payload::default(); 2];
        state.locals[0].address = permission(HoldRoot::Parameter(0), 0);
        let locations = Permission {
            sources: vec![
                source(HoldRoot::Local(LocalId(0))),
                source(HoldRoot::Local(LocalId(1))),
            ],
            loans: vec![],
        };
        assert!(
            read(&state, &locations, &mut Budget::new(&Limits::default()))
                .unwrap()
                .sources
                .is_empty()
        );
    }
    #[test]
    fn call_after_uses_pre_call_data_and_keeps_extra_head_only_as_parent() {
        let (module, _) = crate::tests::baseline();
        let f = &module.functions[0];
        let instruction = &f.blocks[0].instructions[0];
        let mut state = Frame::default();
        state.locals = vec![Payload::default(); 3];
        state.locals[0].address = permission(HoldRoot::Local(LocalId(1)), 0);
        let values = vec![
            Payload {
                address: permission(HoldRoot::Local(LocalId(0)), 1),
                ..Payload::default()
            },
            Payload {
                address: permission(HoldRoot::Parameter(1), 2),
                ..Payload::default()
            },
            Payload {
                address: permission(HoldRoot::Parameter(2), 3),
                ..Payload::default()
            },
        ];
        let summary = Summary {
            complete: true,
            reference_after: After(vec![Output {
                location: source(HoldRoot::Parameter(0)),
                value: pointer(),
                sources: vec![source(HoldRoot::Parameter(1))],
                parents: vec![source(HoldRoot::Parameter(2))],
                stable: false,
            }]),
            ..Summary::default()
        };
        let mut loans = vec![Loan::default(); 4];
        let mut sites = dyn_leases::Sites::default();
        let mut b = Budget::new(&Limits::default());
        let prepared = prepare(
            &module,
            f,
            instruction,
            &summary,
            &[ValueId(0), ValueId(1), ValueId(2)],
            None,
            &values,
            &state,
            &mut sites,
            &mut loans,
            &mut b,
        )
        .unwrap();
        assert_eq!(
            state.locals[0].address.sources,
            vec![source(HoldRoot::Local(LocalId(1)))]
        );
        publish(&mut state, prepared, &mut b).unwrap();
        assert_eq!(
            state.locals[0].address.sources,
            vec![source(HoldRoot::Parameter(1))]
        );
        let ancestors = ancestry(&state.locals[0].address.loans, &loans, &mut b).unwrap();
        assert!(ancestors.contains(&2) && ancestors.contains(&3));
        assert!(
            !state.locals[0]
                .address
                .sources
                .contains(&source(HoldRoot::Parameter(2)))
        );
    }
}

#[cfg(test)]
mod kernel_entry_tests{
    use super::*;
    #[test]
    fn actual_nested_borrow_read_and_write_keep_precise_types_and_modes(){
        let(module,_)=crate::tests::baseline();let mut f=module.functions[0].clone();f.locals.truncate(1);f.locals[0].ty=Type::Address{mutable:false,inner:Box::new(Type::I32)};f.locals[0].mutable=true;
        let shared=f.locals[0].ty.clone();let outer=Type::Address{mutable:true,inner:Box::new(shared.clone())};f.values=vec![outer,shared.clone(),shared.clone(),Type::Address{mutable:false,inner:Box::new(shared)}];
        let span=f.span;let read=Instruction{result:Some(ValueId(2)),kind:InstructionKind::ReadAddress{address:ValueId(0)},span};
        let operations=[Instruction{result:Some(ValueId(0)),kind:InstructionKind::BorrowLocalMut{local:LocalId(0)},span},Instruction{result:None,kind:InstructionKind::WriteAddress{address:ValueId(0),value:ValueId(1)},span},read.clone(),Instruction{result:Some(ValueId(3)),kind:InstructionKind::BorrowLocal{local:LocalId(0)},span}];
        let mut b=Budget::new(&Limits::default());b.raw_reference_cells=true;b.stored_borrows=true;
        for instruction in operations{crate::check_instruction(&module,&f,&instruction,&mut b).unwrap();}
        f.values[2]=Type::Address{mutable:true,inner:Box::new(Type::I32)};
        assert!(crate::check_instruction(&module,&f,&read,&mut b).is_err(),"a shared stored pointer cannot be read as an exclusive pointer");
    }
}
