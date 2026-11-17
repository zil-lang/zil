//! Complete actual inline value transport. Physical paths locate occupants;
//! they never identify a logical class or manufacture a pointer. Root-only
//! facts retain an explicit field location instead of disappearing on nesting.
use super::*;
mod after;
mod input_environments;
mod inputs;
pub(super) use input_environments::seed_environments;
mod results;
pub(super) use after::{
    After, PreparedAfter, after_sites, after_sources, after_work, prepare_after, publish_after,
    summarize_after,
};
pub(super) use inputs::{seed_capture, seed_parameter};
pub(super) use results::{
    ResultFacts, SymbolicPermission, required_channels_known, result_bound, result_work, summarize,
    validate_results,
};
pub(super) fn memory_after_sources(
    module: &TypedModule,
    summary: &Summary,
    b: &mut Budget,
) -> Check<Vec<(HoldRoot, Path, Vec<Source>)>> {
    let mut rows = after_sources(module, &summary.value_after, b)?;
    for (base, path, sources) in summary
        .external_after
        .iter()
        .map(|(index, path, sources)| (HoldRoot::Parameter(*index), path, sources))
        .chain(
            summary
                .capture_after
                .iter()
                .map(|(index, path, sources)| (HoldRoot::Capture(*index), path, sources)),
        )
    {
        b.spend(rows.len().saturating_mul(path.len().saturating_add(1)))?;
        if let Some((_, _, known)) = rows
            .iter_mut()
            .find(|(root, key, _)| *root == base && key == path)
        {
            add_sources(known, sources, b)?;
        } else {
            b.spend(path.len().saturating_add(1))?;
            source_work(sources, b)?;
            rows.push((base, path.clone(), sources.clone()));
        }
    }
    Ok(rows)
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct FieldFacts {
    pub(super) address: Permission,
    pub(super) description: Permission,
    pub(super) views: Vec<dyn_ports::View>,
    pub(super) option: option_address::Variants,
    pub(super) creation: Option<bool>,
}
impl FieldFacts {
    fn present(&self) -> bool {
        !self.address.sources.is_empty()
            || !self.address.loans.is_empty()
            || !self.description.sources.is_empty()
            || !self.description.loans.is_empty()
            || !self.views.is_empty()
            || self.option.mask() != 0
            || self.creation.is_some()
    }
    pub(super) fn work(&self, b: &mut Budget) -> Check<()> {
        permission_work(&self.address, b)?;
        permission_work(&self.description, b)?;
        dyn_ports::views_work(&self.views, b)?;
        self.option.work(b)?;
        b.spend(1)
    }
    fn join(&mut self, other: &Self, b: &mut Budget) -> Check<bool> {
        let mut changed = join_permission(&mut self.address, &other.address, b)?
            | join_permission(&mut self.description, &other.description, b)?;
        for view in &other.views {
            dyn_ports::views_work(&self.views, b)?;
            permission_work(&view.head, b)?;
            if !self.views.contains(view) {
                b.spend(1)?;
                self.views.push(view.clone());
                changed = true;
            }
        }
        changed |= self.option.join(&other.option, b)?;
        if let Some(complete) = other.creation {
            let joined = Some(self.creation.unwrap_or(false) || complete);
            changed |= self.creation != joined;
            self.creation = joined;
        }
        Ok(changed)
    }
}
pub(super) fn facts<'a>(
    payload: &'a Payload,
    path: &[PlaceProjection],
    b: &mut Budget,
) -> Check<Option<&'a FieldFacts>> {
    b.spend(
        payload
            .field_facts
            .len()
            .saturating_mul(path.len().saturating_add(1)),
    )?;
    let mut rows = payload.field_facts.iter().filter(|(key, _)| key == path);
    let row = rows.next();
    if rows.next().is_some() {
        return Err(err("actual field facts repeat one physical occupant"));
    }
    Ok(row.map(|(_, facts)| facts))
}
pub(super) fn put(
    payload: &mut Payload,
    path: &[PlaceProjection],
    facts: &FieldFacts,
    b: &mut Budget,
) -> Check<bool> {
    if path.is_empty() {
        let mut changed = join_permission(&mut payload.address, &facts.address, b)?
            | join_permission(&mut payload.store_description, &facts.description, b)?;
        for view in &facts.views {
            dyn_ports::views_work(&payload.dyn_views, b)?;
            permission_work(&view.head, b)?;
            if !payload.dyn_views.contains(view) {
                b.spend(1)?;
                payload.dyn_views.push(view.clone());
                changed = true;
            }
        }
        changed |= payload.option_variants.join(&facts.option, b)?;
        if let Some(complete) = facts.creation {
            let joined = Some(payload.creation_complete.unwrap_or(false) || complete);
            changed |= payload.creation_complete != joined;
            payload.creation_complete = joined;
        }
        return Ok(changed);
    }
    if !facts.present() {
        return Ok(false);
    }
    b.spend(
        payload
            .field_facts
            .len()
            .saturating_mul(path.len().saturating_add(1)),
    )?;
    if let Some((_, old)) = payload.field_facts.iter_mut().find(|(key, _)| key == path) {
        old.join(facts, b)
    } else {
        facts.work(b)?;
        b.spend(path.len().saturating_add(1))?;
        payload.field_facts.push((path.to_vec(), facts.clone()));
        Ok(true)
    }
}
pub(super) fn work(payload: &Payload, b: &mut Budget) -> Check<()> {
    b.spend(payload.field_facts.len())?;
    for (path, facts) in &payload.field_facts {
        b.spend(path.len())?;
        facts.work(b)?;
    }
    Ok(())
}
pub(super) fn join(out: &mut Payload, from: &Payload, b: &mut Budget) -> Check<bool> {
    let mut changed = false;
    for (path, facts) in &from.field_facts {
        changed |= put(out, path, facts, b)?;
    }
    Ok(changed)
}
/// One exhaustive visitor owns all root-only field permissions. Head/view
/// ancestry is visited as ancestry, never reclassified as physical data.
pub(super) fn visit_fields(
    payload: &Payload,
    b: &mut Budget,
    mut visit: impl FnMut(&Permission, &mut Budget) -> Check<()>,
) -> Check<()> {
    for (path, facts) in &payload.field_facts {
        b.spend(path.len().saturating_add(1))?;
        let FieldFacts {
            address,
            description,
            views,
            option: _,
            creation: _,
        } = facts;
        visit(address, b)?;
        visit(description, b)?;
        for view in views {
            visit(&view.head, b)?;
        }
    }
    Ok(())
}
pub(super) fn visit_permissions(
    payload: &Payload,
    b: &mut Budget,
    mut visit: impl FnMut(&Permission, &mut Budget) -> Check<()>,
) -> Check<()> {
    let Payload {
        address,
        store_description,
        leaves,
        task_ports,
        dyn_ports,
        dyn_views,
        field_facts: _,
        enum_variants: _,
        option_variants: _,
        creation_complete: _,
    } = payload;
    visit(address, b)?;
    visit(store_description, b)?;
    for (path, permission) in leaves {
        b.spend(path.len())?;
        visit(permission, b)?;
    }
    for (path, permission) in task_ports {
        b.spend(path.len())?;
        visit(permission, b)?;
    }
    for (path, port) in &dyn_ports.0 {
        b.spend(path.len())?;
        visit(&port.permission, b)?;
    }
    for view in dyn_views {
        visit(&view.head, b)?;
    }
    visit_fields(payload, b, &mut visit)
}
fn contract(path: &[PlaceProjection], b: &mut Budget) -> Check<Vec<ContractProjection>> {
    b.spend(path.len())?;
    Ok(path
        .iter()
        .copied()
        .map(ContractProjection::Place)
        .collect())
}
/// Install an already checked child, carrying every channel. Constructors do
/// not convert a missing source into an input or a Static literal.
pub(super) fn attach(
    out: &mut Payload,
    path: &[PlaceProjection],
    ty: &Type,
    child: &Payload,
    b: &mut Budget,
) -> Check<()> {
    if path.is_empty() {
        join_payload(out, child, b)?;
        return Ok(());
    }

    let Payload {
        enum_variants,
        option_variants,
        address,
        store_description,
        task_ports,
        dyn_ports,
        dyn_views,
        creation_complete,
        leaves,
        field_facts,
    } = child;
    out.enum_variants.replace_child(path, enum_variants, b)?;
    let pointer = matches!(ty, Type::Address { .. } | Type::Desc(_));
    if pointer {
        add_leaf(out, path, address, b)?;
    }
    let root = FieldFacts {
        address: if pointer {
            Permission::default()
        } else {
            permission_work(address, b)?;
            address.clone()
        },
        description: {
            permission_work(store_description, b)?;
            store_description.clone()
        },
        views: {
            dyn_ports::views_work(dyn_views, b)?;
            dyn_views.clone()
        },
        option: {
            option_variants.work(b)?;
            option_variants.clone()
        },
        creation: *creation_complete,
    };
    put(out, path, &root, b)?;
    for (tail, permission) in leaves {
        b.spend(path.len().saturating_add(tail.len()))?;
        let mut target = path.to_vec();
        target.extend(tail);
        add_leaf(out, &target, permission, b)?;
    }
    for (tail, facts) in field_facts {
        b.spend(path.len().saturating_add(tail.len()))?;
        let mut target = path.to_vec();
        target.extend(tail);
        put(out, &target, facts, b)?;
    }
    let prefix = contract(path, b)?;
    for (tail, permission) in task_ports {
        b.spend(prefix.len().saturating_add(tail.len()))?;
        let mut target = prefix.clone();
        target.extend(tail);
        task_address::insert(out, &target, permission, b)?;
    }
    for (tail, port) in &dyn_ports.0 {
        b.spend(prefix.len().saturating_add(tail.len()))?;
        let mut target = prefix.clone();
        target.extend(tail);
        dyn_ports::insert(&mut out.dyn_ports, &target, port, b)?;
    }
    Ok(())
}
pub(super) fn project(
    source: &Payload,
    path: &[PlaceProjection],
    ty: &Type,
    b: &mut Budget,
) -> Check<Payload> {
    if path.is_empty() {
        return copy_payload(source, b);
    }
    let mut out = Payload::default();
    out.enum_variants = source.enum_variants.project(path, b)?;
    for (key, permission) in &source.leaves {
        b.spend(key.len().saturating_add(path.len()))?;
        if let Some(tail) = key.strip_prefix(path) {
            if tail.is_empty() && matches!(ty, Type::Address { .. } | Type::Desc(_)) {
                join_permission(&mut out.address, permission, b)?;
            } else {
                add_leaf(&mut out, tail, permission, b)?;
            }
        }
    }
    for (key, row) in &source.field_facts {
        b.spend(key.len().saturating_add(path.len()))?;
        if let Some(tail) = key.strip_prefix(path) {
            if tail.is_empty() {
                row.work(b)?;
                join_permission(&mut out.address, &row.address, b)?;
                join_permission(&mut out.store_description, &row.description, b)?;
                out.dyn_views = row.views.clone();
                out.option_variants = row.option.clone();
                out.creation_complete = row.creation;
            } else {
                put(&mut out, tail, row, b)?;
            }
        }
    }
    let prefix = contract(path, b)?;
    for (key, permission) in &source.task_ports {
        b.spend(key.len().saturating_add(prefix.len()))?;
        if let Some(tail) = key.strip_prefix(prefix.as_slice()) {
            task_address::insert(&mut out, tail, permission, b)?;
        }
    }
    for (key, port) in &source.dyn_ports.0 {
        b.spend(key.len().saturating_add(prefix.len()))?;
        if let Some(tail) = key.strip_prefix(prefix.as_slice()) {
            dyn_ports::insert(&mut out.dyn_ports, tail, port, b)?;
        }
    }
    Ok(out)
}
pub(super) fn remove(out: &mut Payload, path: &[PlaceProjection], b: &mut Budget) -> Check<()> {
    if path.is_empty() {
        *out = Payload::default();
        return Ok(());
    }

    out.enum_variants
        .replace_child(path, &enum_address::Variants::default(), b)?;
    out.leaves.retain(|(key, _)| !key.starts_with(path), b)?;
    b.spend(out.field_facts.iter().fold(0usize, |n, (key, _)| {
        n.saturating_add(key.len().saturating_add(path.len()).saturating_add(1))
    }))?;
    out.field_facts.retain(|(key, _)| !key.starts_with(path));
    let prefix = contract(path, b)?;
    b.spend(out.task_ports.iter().fold(0usize, |n, (key, _)| {
        n.saturating_add(key.len().saturating_add(prefix.len()).saturating_add(1))
    }))?;
    out.task_ports.retain(|(key, _)| !key.starts_with(&prefix));
    b.spend(out.dyn_ports.0.iter().fold(0usize, |n, (key, _)| {
        n.saturating_add(key.len().saturating_add(prefix.len()).saturating_add(1))
    }))?;
    out.dyn_ports.0.retain(|(key, _)| !key.starts_with(&prefix));
    Ok(())
}
pub(super) fn replace(
    out: &mut Payload,
    path: &[PlaceProjection],
    ty: &Type,
    child: &Payload,
    b: &mut Budget,
) -> Check<()> {
    if path.is_empty() {
        *out = copy_payload(child, b)?;
        return Ok(());
    }
    remove(out, path, b)?;
    attach(out, path, ty, child, b)
}
/// Structural child selection is not a layout query across a pointer.
pub(super) fn field_type(mut ty: &Type, index: u32) -> Check<&Type> {
    while let Type::BorrowedStorage { storage, .. } = ty {
        ty = storage;
    }
    match ty {
        Type::Tuple(fields) => fields.get(index as usize),
        Type::Record { fields, .. } | Type::Package { fields, .. } => {
            fields.get(index as usize).map(|(_, ty)| ty)
        }
        Type::Array { element, length } if index < *length => Some(element.as_ref()),
        _ => None,
    }
    .ok_or_else(|| err("value transport selects no actual inline field"))
}
pub(super) fn take(
    holder: &mut Payload,
    path: &[PlaceProjection],
    ty: &Type,
    b: &mut Budget,
) -> Check<Payload> {
    let value = project(holder, path, ty, b)?;
    remove(holder, path, b)?;
    Ok(value)
}
pub(super) fn option_at(
    payload: &Payload,
    path: &[PlaceProjection],
    b: &mut Budget,
) -> Check<option_address::Variants> {
    let value = if path.is_empty() {
        &payload.option_variants
    } else {
        match facts(payload, path, b)? {
            Some(facts) => &facts.option,
            None => return Ok(option_address::Variants::default()),
        }
    };
    value.work(b)?;
    Ok(value.clone())
}
pub(super) fn views_at<'a>(
    payload: &'a Payload,
    path: &[ContractProjection],
    b: &mut Budget,
) -> Check<&'a [dyn_ports::View]> {
    if path.is_empty() {
        return Ok(&payload.dyn_views);
    }
    b.spend(path.len())?;
    let physical = path
        .iter()
        .map(|step| match step {
            ContractProjection::Place(step) => Ok(*step),
            ContractProjection::Capture(index) => Ok(PlaceProjection::Field(*index)),
            _ => Err(err(
                "view carrier is not an actual inline physical position",
            )),
        })
        .collect::<Check<Vec<_>>>()?;
    Ok(facts(payload, &physical, b)?.map_or(&[], |facts| facts.views.as_slice()))
}

/// Capture's existing pointer/opaque kernels reconstruct current data and
/// retained ports. This adds only the remaining root-only value facts, avoiding
/// a stale second copy of a mutable pointee's data directory.
pub(super) fn attach_aux(
    out: &mut Payload,
    path: &[PlaceProjection],
    ty: &Type,
    child: &Payload,
    b: &mut Budget,
) -> Check<()> {
    let pointer = matches!(ty, Type::Address { .. } | Type::Desc(_));
    let root = FieldFacts {
        address: if pointer {
            Permission::default()
        } else {
            permission_work(&child.address, b)?;
            child.address.clone()
        },
        description: {
            permission_work(&child.store_description, b)?;
            child.store_description.clone()
        },
        views: {
            dyn_ports::views_work(&child.dyn_views, b)?;
            child.dyn_views.clone()
        },
        option: {
            child.option_variants.work(b)?;
            child.option_variants.clone()
        },
        creation: child.creation_complete,
    };
    put(out, path, &root, b)?;
    if !pointer {
        for (tail, facts) in &child.field_facts {
            b.spend(path.len().saturating_add(tail.len()))?;
            let mut target = path.to_vec();
            target.extend(tail);
            put(out, &target, facts, b)?;
        }
    }
    Ok(())
}
/// An address denotes current storage, not a frozen copy of its fields.
/// A real ReadAddress operation materializes one full by-value snapshot.
pub(super) fn read(
    state: &Frame,
    head: &Permission,
    ty: &Type,
    module: &TypedModule,
    b: &mut Budget,
) -> Check<(Payload, bool)> {
    let mut out = Payload::default();
    let mut known = !head.sources.is_empty();
    for source in &head.sources {
        if source.retained_port.is_some()
            || source.heap_element.is_some()
            || !source.heap_parents.is_empty()
            || !source.leaf.is_empty()
        {
            if ty.is_copy_value() {
                continue;
            } // No stored fact channel; access is checked independently.
            return Err(err(
                "memory read requires its independent opaque, heap or saved-target route",
            ));
        }
        if source.base == HoldRoot::Static && ty.is_copy_value() {
            continue;
        }
        let holder = memory_holder(state, source.base, b)?
            .ok_or_else(|| err("memory read has no actual initialized holder"))?;
        let value = project(holder, &source.inline, ty, b)?;
        known &= required_channels_known(module, ty, &value, b)?;
        join_payload(&mut out, &value, b)?;
    }
    Ok((out, known))
}
pub(super) fn write(
    state: &mut Frame,
    head: &Permission,
    ty: &Type,
    rhs: &Payload,
    module: &TypedModule,
    b: &mut Budget,
) -> Check<()> {
    if head.sources.is_empty() || !required_channels_known(module, ty, rhs, b)? {
        state.pending_external_update = true;
        return Ok(());
    }
    let strong = head.sources.len() == 1;
    for source in &head.sources {
        source_work(std::slice::from_ref(source), b)?;
        if source.retained_port.is_some()
            || source.heap_element.is_some()
            || !source.heap_parents.is_empty()
            || !source.leaf.is_empty()
        {
            if ty.is_copy_value() {
                continue;
            } // Pure data has no stored permission/auxiliary state.
            return Err(err(
                "memory write requires its independent opaque, heap or saved-target route",
            ));
        }
        let holder = memory_holder_mut(state, source.base, b)?
            .ok_or_else(|| err("memory write has no actual mutable holder"))?;
        if strong {
            replace(holder, &source.inline, ty, rhs, b)?;
        } else {
            attach_weak(holder, &source.inline, ty, rhs, b)?;
        }
    }
    Ok(())
}

pub(super) fn attach_weak(
    out: &mut Payload,
    path: &[PlaceProjection],
    ty: &Type,
    child: &Payload,
    b: &mut Budget,
) -> Check<()> {
    if path.is_empty() {
        join_payload(out, child, b)?;
        return Ok(());
    }
    let mut prefixed = Payload::default();
    attach(&mut prefixed, path, ty, child, b)?;
    out.enum_variants
        .join_child(path, &child.enum_variants, b)?;
    prefixed.enum_variants = enum_address::Variants::default();
    join_payload(out, &prefixed, b)?;
    Ok(())
}
