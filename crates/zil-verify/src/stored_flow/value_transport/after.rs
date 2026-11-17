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
    _entry: &Frame,
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
        if !auxiliary_type(module, ty, b)? {
            continue;
        }
        let payload = payload.ok_or_else(|| err("auxiliary after-state input holder is absent"))?;
        let (facts, known) =
            super::results::snapshot_rows(module, f, ty, payload, solution, true, b)?;
        summary.complete &= known;
        if matches!(ty, Type::ErrorUnion { .. } | Type::Enum { .. })
            || matches!(ty,Type::BorrowedStorage{storage,..}if matches!(storage.as_ref(),Type::Enum{..}))
        {
            summary.complete &= payload.enum_variants.grounded(ty, b)?;
        }
        super::results::validate_after_rows(module, f, base, through, ty, &facts, b)?;
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
            });
        }
    }
    Ok(())
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
        for local in &_callee.parameters {
            if let Type::Address {
                mutable: true,
                inner,
            } = &_callee.locals[local.0 as usize].ty
            {
                if auxiliary_type(before.module(), inner, b)? {
                    out.pending = true;
                    break;
                }
            }
        }
        if let Some(definition) = _callee
            .environment
            .and_then(|id| before.module().closures.get(id.0 as usize))
        {
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
                    if auxiliary_type(before.module(), ty, b)? {
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
        let (tags, known) = before.enum_result(&row.tags, b)?;
        if !known {
            out.pending = true;
            continue;
        }
        value.enum_variants = tags;
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
}
