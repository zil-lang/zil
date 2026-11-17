//! Auxiliary memory facts use the same immutable call-before source binding.
//! Physical pointer slots and opaque port ledgers keep their separate kernels.
use super::*;
#[derive(Clone, Debug, PartialEq, Eq)]
struct Row {
    base: HoldRoot,
    through: bool,
    ty: Type,
    facts: Vec<(Path, ResultFacts)>,
    tags: enum_address::Variants,
    physical: Vec<(Path, SymbolicPermission, bool)>,
    description_paths: Vec<Path>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(in crate::stored_flow) struct After(Vec<Row>);
#[derive(Default)]
pub(in crate::stored_flow) struct PreparedAfter {
    rows: Vec<(Source, Type, Payload, bool, bool)>,
    pending: bool,
}
pub(in crate::stored_flow) fn after_work(after: &After, b: &mut Budget) -> Check<()> {
    b.spend(after.0.len())?;
    for row in &after.0 {
        type_work(&row.ty, &b.limits.clone(), b)?;
        row.tags.work(b)?;
        for path in &row.description_paths {
            b.spend(path.len().saturating_add(1))?;
        }
        for (path, permission, _) in &row.physical {
            b.spend(path.len())?;
            source_work(&permission.sources, b)?;
            source_work(&permission.parents, b)?;
            b.spend(1)?;
        }
        for (path, facts) in &row.facts {
            b.spend(path.len())?;
            result_work(facts, b)?;
        }
    }
    Ok(())
}
fn auxiliary_type(module: &TypedModule, ty: &Type, b: &mut Budget) -> Check<bool> {
    let mut pending = vec![(ty, 0usize)];
    while let Some((ty, depth)) = pending.pop() {
        b.spend(1)?;
        if depth > b.limits.max_call_depth.min(128) {
            return Err(limit("auxiliary after-state type depth exceeded"));
        }
        if option_address::signature(ty)
            || matches!(
                ty,
                Type::Desc(_)
                    | Type::Dyn { .. }
                    | Type::OwnedDyn { .. }
                    | Type::Managed(_)
                    | Type::Task { .. }
            )
            || matches!(ty,Type::Address{inner,..}if matches!(inner.as_ref(),Type::Dyn{..}|Type::OwnedDyn{..}))
        {
            return Ok(true);
        }
        match ty {
            Type::BorrowedStorage { storage, .. } => pending.push((storage, depth + 1)),
            Type::Tuple(fields) => {
                b.spend(fields.len())?;
                pending.extend(fields.iter().map(|ty| (ty, depth + 1)));
            }
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                b.spend(fields.len())?;
                pending.extend(fields.iter().map(|(_, ty)| (ty, depth + 1)));
            }
            Type::Enum { .. } | Type::ErrorUnion { .. } => return Ok(true),
            Type::Array { element, .. } => pending.push((element, depth + 1)),
            Type::Closure(id) => {
                let definition = module
                    .closures
                    .get(id.0 as usize)
                    .filter(|d| d.id == *id)
                    .ok_or_else(|| err("auxiliary after-state closure identity is absent"))?;
                b.spend(definition.captures.len())?;
                pending.extend(definition.captures.iter().map(|ty| (ty, depth + 1)));
            }
            _ => {}
        }
    }
    Ok(false)
}
/// Physical pointer and descriptor fields. A root Address is a separate
/// reference cell and never obtains a duplicate after-state row here.
fn physical_paths(module: &TypedModule, ty: &Type, b: &mut Budget) -> Check<Vec<(Path, bool)>> {
    let mut pending = vec![(ty, Path::new(), 0usize)];
    let mut out = vec![];
    while let Some((ty, path, depth)) = pending.pop() {
        b.spend(path.len().saturating_add(1))?;
        if depth > b.limits.max_call_depth.min(128) {
            return Err(limit("physical after-state directory depth exceeded"));
        }
        match ty {
            Type::Address { mutable, .. } => {
                if !path.is_empty() {
                    b.spend(1)?;
                    out.push((path, *mutable));
                }
            }
            Type::Desc(_) => {
                if !path.is_empty() {
                    b.spend(1)?;
                    out.push((path, true));
                }
            }
            Type::BorrowedStorage { life, .. } => {
                for slot in &life.slots {
                    b.spend(path.len().saturating_add(slot.path.len()).saturating_add(1))?;
                    let mut key = path.clone();
                    key.extend(&slot.path);
                    if !key.is_empty() {
                        out.push((key, slot.mode == StoredBorrowMode::ExclusiveAddress));
                    }
                }
            }
            Type::Closure(id) => {
                for (key, ty) in returned_environment::leaves(module, *id, b)? {
                    if matches!(ty, Type::Address { .. } | Type::Desc(_)) {
                        b.spend(path.len().saturating_add(key.len()).saturating_add(1))?;
                        let mut full = path.clone();
                        full.extend(key);
                        out.push((
                            full,
                            matches!(ty, Type::Address { mutable: true, .. } | Type::Desc(_)),
                        ));
                    }
                }
            }
            _ => {
                let mut fields = vec![];
                match ty {
                    Type::Tuple(children) => {
                        b.spend(children.len())?;
                        fields.extend(
                            children
                                .iter()
                                .enumerate()
                                .map(|(i, t)| (PlaceProjection::Field(i as u32), t)),
                        );
                    }
                    Type::Record {
                        fields: children, ..
                    }
                    | Type::Package {
                        fields: children, ..
                    } => {
                        b.spend(children.len())?;
                        fields.extend(
                            children
                                .iter()
                                .enumerate()
                                .map(|(i, (_, t))| (PlaceProjection::Field(i as u32), t)),
                        );
                    }
                    Type::Enum { variants, .. } => {
                        for (v, (_, children)) in variants.iter().enumerate() {
                            b.spend(children.len())?;
                            fields.extend(children.iter().enumerate().map(|(i, t)| {
                                (
                                    PlaceProjection::Variant(VariantProjection {
                                        variant: v as u32,
                                        index: i as u32,
                                    }),
                                    t,
                                )
                            }));
                        }
                    }
                    Type::ErrorUnion { error, result } => {
                        b.spend(2)?;
                        fields.extend([
                            (PlaceProjection::ErrorArm(true), error.as_ref()),
                            (PlaceProjection::ErrorArm(false), result.as_ref()),
                        ]);
                    }
                    Type::Array { element, length } => {
                        b.spend(*length as usize)?;
                        fields.extend(
                            (0..*length).map(|i| (PlaceProjection::Field(i), element.as_ref())),
                        );
                    }
                    _ => {}
                }
                b.spend(fields.len().saturating_mul(path.len().saturating_add(2)))?;
                for (step, child) in fields {
                    let mut key = path.clone();
                    key.push(step);
                    pending.push((child, key, depth + 1));
                }
            }
        }
    }
    Ok(out)
}
fn transport_type(module: &TypedModule, ty: &Type, b: &mut Budget) -> Check<bool> {
    Ok(auxiliary_type(module, ty, b)? || !physical_paths(module, ty, b)?.is_empty())
}
fn physical_at<'a>(
    module: &TypedModule,
    ty: &Type,
    payload: &'a Payload,
    path: &[PlaceProjection],
    b: &mut Budget,
) -> Check<Option<&'a Permission>> {
    if let Some(permission) = leaf(payload, path, b)? {
        if !permission.sources.is_empty() {
            return Ok(Some(permission));
        }
    }
    if matches!(
        call_results::destination(module, ty, path, b)?,
        Type::Desc(_)
    ) {
        return Ok(facts(payload, path, b)?.map(|facts| &facts.description));
    }
    Ok(None)
}
pub(in crate::stored_flow) fn after_sites(
    module: &TypedModule,
    callee: &TypedFunction,
    b: &mut Budget,
) -> Check<Vec<(u32, Path, bool)>> {
    let mut rows = vec![];
    b.spend(callee.parameters.len())?;
    for (index, local) in callee.parameters.iter().enumerate() {
        if let Type::Address {
            mutable: true,
            inner,
        } = &callee.locals[local.0 as usize].ty
        {
            for (path, exclusive) in physical_paths(module, inner, b)? {
                b.spend(1)?;
                rows.push((index as u32, path, exclusive));
            }
        }
    }
    if let Some(definition) = callee
        .environment
        .and_then(|id| module.closures.get(id.0 as usize))
    {
        b.spend(definition.captures.len())?;
        for (index, ty) in definition.captures.iter().enumerate() {
            let ty = match ty {
                Type::Address {
                    mutable: true,
                    inner,
                } => inner.as_ref(),
                _ if definition.kind == CallableKind::FnMut => ty,
                _ => continue,
            };
            for (path, exclusive) in physical_paths(module, ty, b)? {
                b.spend(path.len().saturating_add(2))?;
                let mut key = vec![PlaceProjection::Field(index as u32)];
                key.extend(path);
                rows.push((u32::MAX, key, exclusive));
            }
        }
    }
    Ok(rows)
}
fn active_row(
    module: &TypedModule,
    row: &Row,
    path: &[PlaceProjection],
    b: &mut Budget,
) -> Check<bool> {
    row.tags.work(b)?;
    let mut payload = Payload {
        enum_variants: row.tags.clone(),
        ..Default::default()
    };
    for (key, facts) in &row.facts {
        facts.option.work(b)?;
        if key.is_empty() {
            payload.option_variants = facts.option.clone();
        } else {
            put(
                &mut payload,
                key,
                &FieldFacts {
                    option: facts.option.clone(),
                    ..Default::default()
                },
                b,
            )?;
        }
    }
    call_results::path_active(module, &row.ty, &payload, path, b)
}
fn channels_known(expected: &ResultFacts, actual: Option<&ResultFacts>) -> bool {
    let Some(actual) = actual else {
        return !(!expected.address.sources.is_empty()
            || !expected.description.sources.is_empty()
            || !expected.views.is_empty()
            || expected.option.mask() != 0
            || expected.creation.is_some());
    };
    (expected.address.sources.is_empty() || !actual.address.sources.is_empty())
        && (expected.description.sources.is_empty() || !actual.description.sources.is_empty())
        && (expected.views.is_empty() || !actual.views.is_empty())
        && (expected.option.mask() == 0 || actual.option.mask() != 0)
        && (expected.creation.is_none() || actual.creation.is_some())
}
pub(in crate::stored_flow) fn summarize_after(
    module: &TypedModule,
    f: &TypedFunction,
    entry: &Frame,
    state: &Frame,
    solution: &Solution,
    summary: &mut Summary,
    b: &mut Budget,
) -> Check<()> {
    let mut inputs = vec![];
    for (index, local) in f.parameters.iter().enumerate() {
        b.spend(1)?;
        if let Type::Address {
            mutable: true,
            inner,
        } = &f.locals[local.0 as usize].ty
        {
            inputs.push((
                HoldRoot::Parameter(index as u32),
                true,
                inner.as_ref(),
                memory_holder(state, HoldRoot::Parameter(index as u32), b)?,
            ));
        }
    }
    if let Some(definition) = f
        .environment
        .and_then(|id| module.closures.get(id.0 as usize))
    {
        for (index, ty) in definition.captures.iter().enumerate() {
            b.spend(1)?;
            match ty {
                Type::Address {
                    mutable: true,
                    inner,
                } => inputs.push((
                    HoldRoot::Capture(index as u32),
                    true,
                    inner.as_ref(),
                    memory_holder(state, HoldRoot::Capture(index as u32), b)?,
                )),
                _ if definition.kind == CallableKind::FnMut => inputs.push((
                    HoldRoot::Capture(index as u32),
                    false,
                    ty,
                    state.captures.get(index),
                )),
                _ => {}
            }
        }
    }
    for (base, through, ty, payload) in inputs {
        if !transport_type(module, ty, b)? {
            continue;
        }
        let payload = payload.ok_or_else(|| err("auxiliary after-state input holder is absent"))?;
        let (facts, known) =
            super::results::snapshot_rows(module, f, ty, payload, solution, true, b)?;
        summary.complete &= known;
        let initial = if through {
            memory_holder(entry, base, b)?
        } else {
            match base {
                HoldRoot::Capture(index) => entry.captures.get(index as usize),
                _ => None,
            }
        };
        let mut physical = vec![];
        for (path, _) in physical_paths(module, ty, b)? {
            if !call_results::path_active(module, ty, payload, &path, b)? {
                continue;
            }
            let Some(permission) = physical_at(module, ty, payload, &path, b)? else {
                summary.complete = false;
                continue;
            };
            if permission.sources.is_empty() {
                summary.complete = false;
                continue;
            }
            let stable = match initial {
                Some(initial) => physical_at(module, ty, initial, &path, b)?
                    .is_some_and(|previous| previous == permission),
                None => false,
            };
            let symbolic = super::results::symbolic(permission, f, solution, b)?;
            // Reuse the full final data/source/parent boundary for each
            // physical field, without turning its parents into targets.
            b.spend(path.len().saturating_add(1))?;
            source_work(&symbolic.sources, b)?;
            source_work(&symbolic.parents, b)?;
            let check = ResultFacts {
                address: symbolic.clone(),
                ..Default::default()
            };
            super::results::validate_after_rows(
                module,
                f,
                base,
                through,
                ty,
                &[(path.clone(), check)],
                b,
            )?;
            b.spend(1)?;
            physical.push((path, symbolic, stable));
        }
        if matches!(ty, Type::ErrorUnion { .. } | Type::Enum { .. })
            || matches!(ty,Type::BorrowedStorage{storage,..}if matches!(storage.as_ref(),Type::Enum{..}))
        {
            summary.complete &= payload.enum_variants.grounded(ty, b)?;
        }
        super::results::validate_after_rows(module, f, base, through, ty, &facts, b)?;
        let mut description_paths = vec![];
        for (path, _) in &facts {
            if matches!(
                call_results::destination(module, ty, path, b)?,
                Type::Desc(_)
            ) {
                b.spend(path.len().saturating_add(1))?;
                description_paths.push(path.clone());
            }
        }

        b.spend(summary.value_after.0.len())?;
        if let Some(old) = summary
            .value_after
            .0
            .iter_mut()
            .find(|row| row.base == base)
        {
            type_work(&old.ty, &b.limits.clone(), b)?;
            type_work(ty, &b.limits.clone(), b)?;
            if old.through != through || old.ty != *ty {
                return Err(err("auxiliary after-state changes its complete input type"));
            }
            for (path, _, _) in &old.physical {
                b.spend(physical.len().saturating_mul(path.len().saturating_add(1)))?;
                if call_results::path_active(module, ty, payload, path, b)?
                    && !physical.iter().any(|(key, _, _)| key == path)
                {
                    summary.complete = false;
                }
            }
            for (path, permission, stable) in physical {
                b.spend(
                    old.physical
                        .len()
                        .saturating_mul(path.len().saturating_add(1)),
                )?;
                if let Some((_, previous, unchanged)) =
                    old.physical.iter_mut().find(|(key, _, _)| *key == path)
                {
                    super::results::join_symbolic(previous, &permission, b)?;
                    *unchanged &= stable;
                } else {
                    if active_row(module, old, &path, b)? {
                        summary.complete = false;
                    }
                    b.spend(path.len().saturating_add(1))?;
                    source_work(&permission.sources, b)?;
                    source_work(&permission.parents, b)?;
                    old.physical.push((path, permission, stable));
                }
            }
            // Every exit must supply each channel; a union is not evidence
            // that a missing Return field was initialized.
            for (path, row) in &old.facts {
                b.spend(facts.len().saturating_mul(path.len().saturating_add(1)))?;
                if call_results::path_active(module, ty, payload, path, b)?
                    && !channels_known(
                        row,
                        facts
                            .iter()
                            .find(|(key, _)| key == path)
                            .map(|(_, row)| row),
                    )
                {
                    summary.complete = false;
                }
            }
            for (path, row) in facts {
                b.spend(old.facts.len().saturating_mul(path.len().saturating_add(1)))?;
                if let Some((_, prior)) = old.facts.iter_mut().find(|(key, _)| *key == path) {
                    if !channels_known(&row, Some(prior)) {
                        summary.complete = false;
                    }
                    super::results::join_result(prior, &row, b)?;
                } else {
                    if active_row(module, old, &path, b)?
                        && (row.option.mask() != 0
                            || !row.address.sources.is_empty()
                            || !row.description.sources.is_empty()
                            || !row.views.is_empty())
                    {
                        summary.complete = false;
                    }
                    b.spend(path.len().saturating_add(1))?;
                    result_work(&row, b)?;
                    old.facts.push((path, row));
                }
            }
            for path in description_paths {
                b.spend(
                    old.description_paths
                        .len()
                        .saturating_mul(path.len().saturating_add(1)),
                )?;
                if !old.description_paths.contains(&path) {
                    b.spend(1)?;
                    old.description_paths.push(path);
                }
            }
            old.tags.join(&payload.enum_variants, b)?;
        } else {
            type_work(ty, &b.limits.clone(), b)?;
            payload.enum_variants.work(b)?;
            b.spend(1)?;
            summary.value_after.0.push(Row {
                base,
                through,
                ty: ty.clone(),
                facts,
                tags: payload.enum_variants.clone(),
                physical,
                description_paths,
            });
        }
    }
    Ok(())
}
/// One directory for actual parameter data after a call. View heads remain
/// ancestry, while both real descriptor channels name the descriptor target.
pub(in crate::stored_flow) fn after_sources(
    _module: &TypedModule,
    after: &After,
    b: &mut Budget,
) -> Check<Vec<(HoldRoot, Path, Vec<Source>)>> {
    let mut out: Vec<(HoldRoot, Path, Vec<Source>)> = vec![];
    for row in &after.0 {
        let base = row.base;
        let mut append = |path: &Path, sources: &[Source], b: &mut Budget| -> Check<()> {
            b.spend(out.len().saturating_mul(path.len().saturating_add(1)))?;
            if let Some((_, _, known)) = out
                .iter_mut()
                .find(|(parameter, key, _)| *parameter == base && key == path)
            {
                add_sources(known, sources, b)?;
            } else {
                b.spend(path.len().saturating_add(1))?;
                source_work(sources, b)?;
                out.push((base, path.clone(), sources.to_vec()));
            }
            Ok(())
        };
        for (path, permission, _) in &row.physical {
            append(path, &permission.sources, b)?;
        }
        for (path, facts) in &row.facts {
            b.spend(
                row.description_paths
                    .len()
                    .saturating_mul(path.len().saturating_add(1)),
            )?;
            if row.description_paths.contains(path) {
                append(path, &facts.address.sources, b)?;
                append(path, &facts.description.sources, b)?;
            }
        }
    }
    Ok(out)
}
pub(in crate::stored_flow) fn prepare_after(
    before: &call_results::Before<'_>,
    _callee: &TypedFunction,
    summary: &Summary,
    i: &Instruction,
    sites: &WriteSites,
    loans: &mut [Loan],
    b: &mut Budget,
) -> Check<PreparedAfter> {
    let mut out = PreparedAfter::default();
    if !summary.complete {
        b.spend(_callee.parameters.len())?;
        for local in &_callee.parameters {
            if let Type::Address {
                mutable: true,
                inner,
            } = &_callee.locals[local.0 as usize].ty
            {
                if transport_type(before.module(), inner, b)? {
                    out.pending = true;
                    break;
                }
            }
        }
        if let Some(definition) = _callee
            .environment
            .and_then(|id| before.module().closures.get(id.0 as usize))
        {
            b.spend(definition.captures.len())?;
            for ty in &definition.captures {
                let target = match ty {
                    Type::Address {
                        mutable: true,
                        inner,
                    } => Some(inner.as_ref()),
                    _ if definition.kind == CallableKind::FnMut => Some(ty),
                    _ => None,
                };
                if let Some(ty) = target {
                    if transport_type(before.module(), ty, b)? {
                        out.pending = true;
                        break;
                    }
                }
            }
        }
        return Ok(out);
    }
    for row in &summary.value_after.0 {
        // A consumed environment has retired as an occupant. Its actual
        // mutable pointee effects still publish, but its own slots do not.
        if !row.through && matches!(row.base, HoldRoot::Capture(_)) && before.consumed_environment()
        {
            continue;
        }
        let mut value = Payload::default();
        let (tags, known) = before.enum_result(&row.tags, b)?;
        if !known {
            out.pending = true;
            continue;
        }
        value.enum_variants = tags;
        let selected = Summary {
            complete: true,
            field_result_facts: {
                for (path, facts) in &row.facts {
                    b.spend(path.len())?;
                    result_work(facts, b)?;
                }
                row.facts.clone()
            },
            ..Default::default()
        };
        if !before.field_results(&selected, &row.ty, i, sites, loans, &mut value, b)? {
            out.pending = true;
            continue;
        }
        let mut physical_known = true;
        for (path, permission, stable) in &row.physical {
            if !call_results::path_active(before.module(), &row.ty, &value, path, b)? {
                continue;
            }
            let description = matches!(
                call_results::destination(before.module(), &row.ty, path, b)?,
                Type::Desc(_)
            );
            let (mut actual, known) =
                before.physical_permission(permission, description, i, sites, loans, b)?;
            physical_known &= known && !actual.sources.is_empty();
            if !known || actual.sources.is_empty() {
                continue;
            }
            if !*stable {
                let (parameter, site) = match row.base {
                    HoldRoot::Parameter(index) => (index, path.clone()),
                    HoldRoot::Capture(index) => {
                        b.spend(path.len().saturating_add(1))?;
                        let mut key = vec![PlaceProjection::Field(index)];
                        key.extend(path);
                        (u32::MAX, key)
                    }
                    _ => return Err(err("physical after-state has no persistent input")),
                };
                actual = retain_written_slot(&actual, i, parameter, &site, sites, loans, b)?;
            }
            add_leaf(&mut value, path, &actual, b)?;
        }
        if !physical_known {
            out.pending = true;
            continue;
        }
        let targets = before.after_targets(row.base, row.through, b)?;
        if targets.sources.is_empty() {
            out.pending = true;
            continue;
        }
        let unique = targets.sources.len() == 1;
        for target in &targets.sources {
            if target.retained_port.is_some()
                || target.heap_element.is_some()
                || !target.heap_parents.is_empty()
                || !target.leaf.is_empty()
                || target.base == HoldRoot::Static
            {
                return Err(err(
                    "auxiliary after-state lacks an exact mutable inline target",
                ));
            }
            source_work(std::slice::from_ref(target), b)?;
            type_work(&row.ty, &b.limits.clone(), b)?;
            payload_work(&value, b)?;
            b.spend(1)?;
            out.rows.push((
                target.clone(),
                row.ty.clone(),
                value.clone(),
                unique,
                !matches!(row.ty, Type::Address { .. }),
            ));
        }
    }
    Ok(out)
}
pub(in crate::stored_flow) fn publish_after(
    state: &mut Frame,
    prepared: PreparedAfter,
    b: &mut Budget,
) -> Check<()> {
    state.pending_external_update |= prepared.pending;
    for (target, _ty, value, strong, root_address) in prepared.rows {
        let holder = memory_holder_mut(state, target.base, b)?
            .ok_or_else(|| err("auxiliary after-state target holder is absent"))?;
        for (path, permission) in &value.leaves {
            b.spend(target.inline.len().saturating_add(path.len()))?;
            let mut full = target.inline.clone();
            full.extend(path);
            if strong && let Some(index) = holder.leaves.position(&full, b)? {
                permission_work(permission, b)?;
                *holder.leaves.permission_mut(index) = permission.clone();
            } else {
                add_leaf(holder, &full, permission, b)?;
            }
        }
        if strong {
            b.spend(
                holder
                    .field_facts
                    .iter()
                    .map(|(path, _)| {
                        path.len()
                            .saturating_add(target.inline.len())
                            .saturating_add(1)
                    })
                    .sum(),
            )?;
            holder
                .field_facts
                .retain(|(path, _)| !path.starts_with(&target.inline));
            if target.inline.is_empty() {
                holder.store_description = Permission::default();
                holder.dyn_views.clear();
                holder.option_variants = option_address::Variants::default();
                holder.creation_complete = None;
            }
        }
        if target.inline.is_empty() {
            if root_address {
                if strong {
                    holder.address = Permission::default();
                }
                join_permission(&mut holder.address, &value.address, b)?;
            }
            join_permission(&mut holder.store_description, &value.store_description, b)?;
            for view in &value.dyn_views {
                dyn_ports::views_work(&holder.dyn_views, b)?;
                permission_work(&view.head, b)?;
                if !holder.dyn_views.contains(view) {
                    b.spend(1)?;
                    holder.dyn_views.push(view.clone());
                }
            }
            holder.option_variants.join(&value.option_variants, b)?;
            if let Some(creation) = value.creation_complete {
                holder.creation_complete =
                    Some(holder.creation_complete.unwrap_or(false) || creation);
            }
            if strong {
                value.enum_variants.work(b)?;
                holder.enum_variants = value.enum_variants.clone();
            } else {
                holder.enum_variants.join(&value.enum_variants, b)?;
            }
            for (path, facts) in &value.field_facts {
                put(holder, path, facts, b)?;
            }
        } else {
            permission_work(&value.address, b)?;
            permission_work(&value.store_description, b)?;
            dyn_ports::views_work(&value.dyn_views, b)?;
            value.option_variants.work(b)?;
            let root = FieldFacts {
                address: value.address.clone(),
                description: value.store_description.clone(),
                views: value.dyn_views.clone(),
                option: value.option_variants.clone(),
                creation: value.creation_complete,
            };
            root.work(b)?;
            put(holder, &target.inline, &root, b)?;
            for (path, facts) in &value.field_facts {
                b.spend(target.inline.len().saturating_add(path.len()))?;
                let mut full = target.inline.clone();
                full.extend(path);
                put(holder, &full, facts, b)?;
            }
            if strong {
                holder
                    .enum_variants
                    .replace_child(&target.inline, &value.enum_variants, b)?;
            } else {
                let mut tags = holder.enum_variants.project(&target.inline, b)?;
                tags.join(&value.enum_variants, b)?;
                holder
                    .enum_variants
                    .replace_child(&target.inline, &tags, b)?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn source(base: HoldRoot, path: Path) -> Source {
        Source {
            base,
            inline: path,
            leaf: vec![],
            retained_port: None,
            heap_parents: vec![],
            heap_element: None,
            static_dependency: StaticProvenance::NotStatic,
        }
    }
    fn permission(index: u32) -> Permission {
        Permission {
            sources: vec![source(HoldRoot::Parameter(index), vec![])],
            loans: vec![],
        }
    }
    #[test]
    fn auxiliary_after_strong_subtree_and_weak_alternative_preserve_physical_and_sibling_facts() {
        let limits = Limits::default();
        for strong in [false, true] {
            let mut b = Budget::new(&limits);
            let mut holder = Payload::default();
            for field in 0..2 {
                put(
                    &mut holder,
                    &[PlaceProjection::Field(field)],
                    &FieldFacts {
                        description: permission(field),
                        ..Default::default()
                    },
                    &mut b,
                )
                .unwrap();
            }
            add_leaf(
                &mut holder,
                &[PlaceProjection::Field(0)],
                &permission(7),
                &mut b,
            )
            .unwrap();
            let value = Payload {
                store_description: permission(2),
                creation_complete: Some(true),
                ..Default::default()
            };
            let mut frame = Frame {
                locals: vec![holder],
                ..Default::default()
            };
            let update = PreparedAfter {
                rows: vec![(
                    source(HoldRoot::Local(LocalId(0)), vec![PlaceProjection::Field(0)]),
                    Type::I32,
                    value,
                    strong,
                    true,
                )],
                pending: false,
            };
            publish_after(&mut frame, update, &mut b).unwrap();
            let holder = &frame.locals[0];
            let updated = facts(holder, &[PlaceProjection::Field(0)], &mut b)
                .unwrap()
                .unwrap();
            assert_eq!(
                updated.description.sources.len(),
                if strong { 1 } else { 2 }
            );
            assert!(
                updated
                    .description
                    .sources
                    .contains(&source(HoldRoot::Parameter(2), vec![]))
            );
            assert_eq!(
                facts(holder, &[PlaceProjection::Field(1)], &mut b)
                    .unwrap()
                    .unwrap()
                    .description
                    .sources,
                permission(1).sources
            );
            assert_eq!(
                leaf(holder, &[PlaceProjection::Field(0)], &mut b)
                    .unwrap()
                    .unwrap()
                    .sources,
                permission(7).sources,
                "physical slots have a separate actual after kernel"
            );
        }
    }
    #[test]
    fn pointee_root_after_replaces_beta_facts_and_preserves_alpha_value() {
        let limits = Limits::default();
        let mut b = Budget::new(&limits);
        let mut holder = Payload {
            address: permission(0),
            store_description: permission(1),
            creation_complete: Some(true),
            ..Default::default()
        };
        put(
            &mut holder,
            &[PlaceProjection::Field(0)],
            &FieldFacts {
                description: permission(2),
                ..Default::default()
            },
            &mut b,
        )
        .unwrap();
        let mut value = Payload::default();
        put(
            &mut value,
            &[PlaceProjection::Field(0)],
            &FieldFacts {
                description: permission(3),
                ..Default::default()
            },
            &mut b,
        )
        .unwrap();
        let alpha = Payload {
            address: permission(7),
            store_description: permission(8),
            creation_complete: Some(true),
            ..Default::default()
        };
        let mut frame = Frame {
            external: vec![alpha],
            pointees: vec![(HoldRoot::Parameter(0), holder)],
            ..Default::default()
        };
        publish_after(
            &mut frame,
            PreparedAfter {
                rows: vec![(
                    source(HoldRoot::Parameter(0), vec![]),
                    Type::Tuple(vec![Type::I32]),
                    value,
                    true,
                    false,
                )],
                pending: false,
            },
            &mut b,
        )
        .unwrap();
        assert_eq!(frame.external[0].address.sources, permission(7).sources);
        assert_eq!(
            frame.external[0].store_description.sources,
            permission(8).sources
        );
        assert_eq!(frame.external[0].creation_complete, Some(true));
        let beta = memory_holder(&frame, HoldRoot::Parameter(0), &mut b)
            .unwrap()
            .unwrap();
        assert!(beta.store_description.sources.is_empty());
        assert_eq!(beta.creation_complete, None);
        assert_eq!(
            facts(beta, &[PlaceProjection::Field(0)], &mut b)
                .unwrap()
                .unwrap()
                .description
                .sources,
            permission(3).sources
        );
    }
    #[test]
    fn raw_aggregate_physical_after_restores_current_beta_fields_and_preserves_stable_loan_identity()
     {
        let (mut module, _) = crate::tests::baseline();
        let mut callee = module.functions[0].clone();
        let pointer = Type::Address {
            mutable: false,
            inner: Box::new(Type::I32),
        };
        let tuple = Type::Tuple(vec![pointer.clone(), pointer.clone()]);
        let carrier = Type::Address {
            mutable: true,
            inner: Box::new(tuple.clone()),
        };
        callee.environment = None;
        callee.foreign = None;
        callee.call_kind = CallKind::Direct;
        callee.parameters = vec![LocalId(0), LocalId(1)];
        callee.locals = vec![
            LocalDecl {
                id: LocalId(0),
                name: "slot".into(),
                ty: carrier.clone(),
                mutable: false,
                span: callee.span,
            },
            LocalDecl {
                id: LocalId(1),
                name: "replacement".into(),
                ty: pointer.clone(),
                mutable: false,
                span: callee.span,
            },
        ];
        module.functions[0] = callee;
        let callee = &module.functions[0];
        let mut caller = callee.clone();
        caller.values = vec![carrier, pointer];
        let limits = Limits::default();
        let mut b = Budget::new(&limits);
        assert!(transport_type(&module, &tuple, &mut b).unwrap());
        assert_eq!(after_sites(&module, callee, &mut b).unwrap().len(), 2);
        let field0 = vec![PlaceProjection::Field(0)];
        let field1 = vec![PlaceProjection::Field(1)];
        let old = |index, loan| Permission {
            sources: vec![source(HoldRoot::Local(LocalId(index)), vec![])],
            loans: vec![loan],
        };
        let mut beta = Payload::default();
        add_leaf(&mut beta, &field0, &old(3, 4), &mut b).unwrap();
        add_leaf(&mut beta, &field1, &old(2, 3), &mut b).unwrap();
        let mut alpha = Payload {
            address: Permission {
                sources: vec![source(HoldRoot::Parameter(0), vec![])],
                loans: vec![0],
            },
            ..Default::default()
        };
        add_leaf(&mut alpha, &field1, &old(3, 4), &mut b).unwrap();
        let replacement = Payload {
            address: old(1, 1),
            ..Default::default()
        };
        let values = vec![alpha.clone(), replacement];
        let mut frame = Frame {
            external: vec![alpha.clone(), Payload::default()],
            pointees: vec![(HoldRoot::Parameter(0), beta)],
            ..Default::default()
        };
        let instruction = Instruction {
            result: None,
            kind: InstructionKind::Call {
                function: callee.id,
                arguments: vec![ValueId(0), ValueId(1)],
            },
            span: callee.span,
        };
        let fresh = SymbolicPermission {
            sources: vec![source(HoldRoot::Parameter(1), vec![])],
            parents: vec![source(HoldRoot::Parameter(0), vec![])],
            receiver: false,
        };
        let mut stable_source = source(HoldRoot::Parameter(0), vec![]);
        stable_source.leaf = field1.clone();
        let stable = SymbolicPermission {
            sources: vec![stable_source],
            ..Default::default()
        };
        let summary = Summary {
            complete: true,
            value_after: After(vec![Row {
                base: HoldRoot::Parameter(0),
                through: true,
                ty: tuple,
                facts: vec![],
                tags: enum_address::Variants::default(),
                physical: vec![
                    (field0.clone(), fresh, false),
                    (field1.clone(), stable, true),
                ],
                description_paths: vec![],
            }]),
            ..Default::default()
        };
        let mut loans = vec![Loan::default(); 5];
        let sites = vec![(
            &instruction as *const Instruction as usize,
            0,
            field0.clone(),
            2,
            false,
        )];
        let before =
            call_results::Before::function(&module, &caller, &instruction, &values, &frame, &mut b)
                .unwrap();
        let prepared = prepare_after(
            &before,
            callee,
            &summary,
            &instruction,
            &sites,
            &mut loans,
            &mut b,
        )
        .unwrap();
        assert!(!prepared.pending);
        publish_after(&mut frame, prepared, &mut b).unwrap();
        let beta = memory_holder(&frame, HoldRoot::Parameter(0), &mut b)
            .unwrap()
            .unwrap();
        let changed = leaf(beta, &field0, &mut b).unwrap().unwrap();
        assert_eq!(changed.sources, old(1, 1).sources);
        assert_eq!(changed.loans, vec![2]);
        assert!(
            loans[2].parents.contains(&0) && loans[2].parents.contains(&1),
            "real carrier/head and data parents survive separately"
        );
        assert_eq!(leaf(beta, &field1, &mut b).unwrap().unwrap(), &old(2, 3));
        assert_eq!(
            frame.external[0].leaves, alpha.leaves,
            "beta publication does not rewrite the pointer SSA snapshot"
        );
    }

    #[test]
    fn public_after_directory_keeps_parameter_capture_and_descriptor_data_roles_separate_from_view_heads()
     {
        let (module, _) = crate::tests::baseline();
        let limits = Limits::default();
        let mut b = Budget::new(&limits);
        let key = vec![PlaceProjection::Field(0)];
        let symbolic = |index| SymbolicPermission {
            sources: vec![source(HoldRoot::Parameter(index), vec![])],
            ..Default::default()
        };
        let facts = ResultFacts {
            address: symbolic(2),
            description: symbolic(3),
            views: vec![(None, symbolic(9))],
            ..Default::default()
        };
        let row = Row {
            base: HoldRoot::Parameter(0),
            through: true,
            ty: Type::Tuple(vec![Type::Desc(Box::new(Type::I32))]),
            facts: vec![(key.clone(), facts)],
            tags: enum_address::Variants::default(),
            physical: vec![(key.clone(), symbolic(1), false)],
            description_paths: vec![key.clone()],
        };
        let capture = Row {
            base: HoldRoot::Capture(0),
            through: true,
            ty: Type::Tuple(vec![Type::Address {
                mutable: false,
                inner: Box::new(Type::I32),
            }]),
            facts: vec![],
            tags: enum_address::Variants::default(),
            physical: vec![(key.clone(), symbolic(4), true)],
            description_paths: vec![],
        };
        let rows = after_sources(&module, &After(vec![row, capture]), &mut b).unwrap();
        let parameter = rows
            .iter()
            .find(|(base, path, _)| *base == HoldRoot::Parameter(0) && path == &key)
            .unwrap();
        assert_eq!(parameter.2.len(), 3);
        assert!(
            !parameter
                .2
                .contains(&source(HoldRoot::Parameter(9), vec![])),
            "view ancestry never becomes a descriptor data target"
        );
        assert!(
            rows.iter()
                .any(|(base, path, sources)| *base == HoldRoot::Capture(0)
                    && path == &key
                    && sources == &symbolic(4).sources)
        );
    }
}
