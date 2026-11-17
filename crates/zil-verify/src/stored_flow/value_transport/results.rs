//! Symbolic exits are rebuilt from every real Return, never from a declared
//! upper. Each channel keeps its own data, parents and invocation-head role.
use super::*;
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(in crate::stored_flow) struct SymbolicPermission {
    pub(in crate::stored_flow) sources: Vec<Source>,
    pub(in crate::stored_flow) parents: Vec<Source>,
    pub(in crate::stored_flow) receiver: bool,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(in crate::stored_flow) struct ResultFacts {
    pub(in crate::stored_flow) address: SymbolicPermission,
    pub(in crate::stored_flow) description: SymbolicPermission,
    pub(in crate::stored_flow) views: Vec<(Option<DynVtableId>, SymbolicPermission)>,
    pub(in crate::stored_flow) option: option_address::Variants,
    pub(in crate::stored_flow) creation: Option<bool>,
}
pub(super) fn symbolic(
    permission: &Permission,
    f: &TypedFunction,
    solution: &Solution,
    b: &mut Budget,
) -> Check<SymbolicPermission> {
    source_work(&permission.sources, b)?;
    let parents = dyn_returns::parents(permission, &solution.loans, b)?;
    dyn_returns::check_parent_escape(f, &parents, b)?;
    Ok(SymbolicPermission {
        sources: permission.sources.clone(),
        parents,
        receiver: receiver::returned_head(permission, solution, b)?,
    })
}
pub(super) fn join_symbolic(
    out: &mut SymbolicPermission,
    from: &SymbolicPermission,
    b: &mut Budget,
) -> Check<()> {
    add_sources(&mut out.sources, &from.sources, b)?;
    add_sources(&mut out.parents, &from.parents, b)?;
    out.receiver |= from.receiver;
    Ok(())
}
pub(in crate::stored_flow) fn result_work(row: &ResultFacts, b: &mut Budget) -> Check<()> {
    for permission in [&row.address, &row.description] {
        source_work(&permission.sources, b)?;
        source_work(&permission.parents, b)?;
        b.spend(1)?;
    }
    b.spend(row.views.len())?;
    for (_, permission) in &row.views {
        source_work(&permission.sources, b)?;
        source_work(&permission.parents, b)?;
    }
    row.option.work(b)?;
    b.spend(1)
}
fn root_facts(ty: &Type, payload: &Payload, b: &mut Budget) -> Check<FieldFacts> {
    permission_work(&payload.store_description, b)?;
    dyn_ports::views_work(&payload.dyn_views, b)?;
    payload.option_variants.work(b)?;
    // The ordinary root address remains in Summary.result/dyn_result_parents;
    // this row owns root auxiliary channels only, without duplicating data.
    Ok(FieldFacts {
        address: if matches!(ty, Type::Address { .. }) {
            Permission::default()
        } else {
            permission_work(&payload.address, b)?;
            payload.address.clone()
        },
        description: payload.store_description.clone(),
        views: payload.dyn_views.clone(),
        option: payload.option_variants.clone(),
        creation: payload.creation_complete,
    })
}
fn record(
    path: &[PlaceProjection],
    facts: &FieldFacts,
    f: &TypedFunction,
    solution: &Solution,
    summary: &mut Summary,
    b: &mut Budget,
) -> Check<()> {
    if !facts.present() {
        return Ok(());
    }
    let mut row = ResultFacts {
        address: symbolic(&facts.address, f, solution, b)?,
        description: symbolic(&facts.description, f, solution, b)?,
        option: {
            facts.option.work(b)?;
            facts.option.clone()
        },
        creation: facts.creation,
        views: vec![],
    };
    for view in &facts.views {
        b.spend(1)?;
        row.views
            .push((view.table, symbolic(&view.head, f, solution, b)?));
    }
    b.spend(
        summary
            .field_result_facts
            .len()
            .saturating_mul(path.len().saturating_add(1)),
    )?;
    if let Some((_, old)) = summary
        .field_result_facts
        .iter_mut()
        .find(|(key, _)| key == path)
    {
        join_symbolic(&mut old.address, &row.address, b)?;
        join_symbolic(&mut old.description, &row.description, b)?;
        for view in row.views {
            result_work(old, b)?;
            source_work(&view.1.sources, b)?;
            source_work(&view.1.parents, b)?;
            if !old.views.contains(&view) {
                b.spend(1)?;
                old.views.push(view);
            }
        }
        old.option.join(&row.option, b)?;
        if let Some(complete) = row.creation {
            old.creation = Some(old.creation.unwrap_or(false) || complete);
        }
    } else {
        result_work(&row, b)?;
        b.spend(path.len().saturating_add(1))?;
        summary.field_result_facts.push((path.to_vec(), row));
    }
    Ok(())
}
pub(in crate::stored_flow) fn summarize(
    module: &TypedModule,
    f: &TypedFunction,
    payload: &Payload,
    solution: &Solution,
    summary: &mut Summary,
    b: &mut Budget,
) -> Check<()> {
    let root = root_facts(&f.return_type, payload, b)?;
    if !required_return_known(module, &f.return_type, payload, &root, b)? {
        summary.complete = false;
    }
    record(&[], &root, f, solution, summary, b)?;
    for (path, facts) in &payload.field_facts {
        if !call_results::path_active(module, &f.return_type, payload, path, b)? {
            continue;
        }
        record(path, facts, f, solution, summary, b)?;
    }
    Ok(())
}
fn nonempty(row: &SymbolicPermission) -> bool {
    !row.sources.is_empty() || !row.parents.is_empty() || row.receiver
}
fn actual<'a>(
    payload: &'a Payload,
    path: &[PlaceProjection],
    root: &'a FieldFacts,
    b: &mut Budget,
) -> Check<Option<&'a FieldFacts>> {
    if path.is_empty() {
        Ok(Some(root))
    } else {
        facts(payload, path, b)
    }
}
/// A union summary is not a proof that a missing branch contained a field.
/// Recheck each requested channel at every independently reachable Return.
pub(in crate::stored_flow) fn validate_returns(
    module: &TypedModule,
    f: &TypedFunction,
    solution: &Solution,
    graph: &FunctionGraph,
    summary: &Summary,
    b: &mut Budget,
) -> Check<()> {
    for (block, reachable) in f.blocks.iter().zip(&graph.reachable) {
        if !reachable {
            continue;
        }
        let Terminator::Return(Some(value)) = block.terminator else {
            continue;
        };
        let payload = solution
            .values
            .get(value.0 as usize)
            .ok_or_else(|| err("result field Return operand is absent"))?;
        let root = root_facts(&f.return_type, payload, b)?;
        required_return(module, &f.return_type, payload, &root, b)?;
        for path in call_results::physical_slots(module, &f.return_type, b)? {
            if !call_results::path_active(module, &f.return_type, payload, &path, b)? {
                continue;
            }
            let permission = if path.is_empty() {
                Some(&payload.address)
            } else {
                leaf(payload, &path, b)?
            };
            if permission.is_none_or(|row| row.sources.is_empty()) {
                return Err(err(
                    "actual Return omits a required initialized physical result",
                ));
            }
        }
        for (path, _) in &summary.result {
            if !call_results::path_active(module, &f.return_type, payload, path, b)? {
                continue;
            }
            let permission = if path.is_empty() {
                Some(&payload.address)
            } else {
                leaf(payload, path, b)?
            };
            if permission.is_none_or(|row| row.sources.is_empty()) {
                return Err(err(
                    "actual Return omits a required result source occurrence",
                ));
            }
        }
        for (path, row) in &summary.field_result_facts {
            b.spend(path.len().saturating_add(1))?;
            if !call_results::path_active(module, &f.return_type, payload, path, b)? {
                continue;
            }
            let current = actual(payload, path, &root, b)?
                .ok_or_else(|| err("result field facts are absent on an actual Return"))?;
            if nonempty(&row.address) && current.address.sources.is_empty()
                || nonempty(&row.description) && current.description.sources.is_empty()
                || !row.views.is_empty() && current.views.is_empty()
                || row.option.mask() != 0 && current.option.mask() == 0
                || row.creation.is_some() && current.creation.is_none()
            {
                return Err(err("result field channel is unknown on an actual Return"));
            }
            if current.creation == Some(false) {
                return Err(err(
                    "result field creation has no completed responsibility proof",
                ));
            }
        }
    }
    Ok(())
}
fn direct_binding(
    f: &TypedFunction,
    base: StorageBase,
    path: &[PlaceProjection],
    b: &mut Budget,
) -> Check<Option<StorageBinder>> {
    let Some(header) = &f.storage_life else {
        return Ok(None);
    };
    b.spend(
        header
            .bindings
            .len()
            .saturating_mul(path.len().saturating_add(1)),
    )?;
    let mut bindings = header.bindings.iter().filter(|row| {
        row.position.base == base
            && row.position.path.len() == path.len()
            && row
                .position
                .path
                .iter()
                .zip(path)
                .all(|(a, b)| *a == ContractProjection::Place(*b))
    });
    let row = bindings.next();
    if bindings.next().is_some() {
        return Err(err(
            "result field has duplicate declared borrow occurrences",
        ));
    }
    Ok(row.map(|row| row.binder))
}
fn declared_bound(
    module: &TypedModule,
    f: &TypedFunction,
    base: StorageBase,
    ty: &Type,
    path: &[PlaceProjection],
    b: &mut Budget,
) -> Check<Option<StorageBinder>> {
    if let Some(bound) = direct_binding(f, base, path, b)? {
        return Ok(Some(bound));
    }
    let Some((class_path, class)) = local_class(module, ty, path, b)? else {
        return Ok(None);
    };
    let Some(header) = &f.storage_life else {
        return Ok(None);
    };
    b.spend(
        header
            .classes
            .len()
            .saturating_mul(class_path.len().saturating_add(1)),
    )?;
    let mut rows = header.classes.iter().filter(|row| {
        row.position.base == base && row.position.path == class_path && row.position.class == class
    });
    let row = rows.next();
    if rows.next().is_some() {
        return Err(err(
            "result field repeats its complete intrinsic occurrence",
        ));
    }
    Ok(row.map(|row| row.binder))
}
pub(in crate::stored_flow) fn result_bound(
    module: &TypedModule,
    f: &TypedFunction,
    path: &[PlaceProjection],
    relations: &mut relation_index::Relations<'_>,
    b: &mut Budget,
) -> Check<StorageBinder> {
    match declared_bound(module, f, StorageBase::Result, &f.return_type, path, b)? {
        Some(bound) => Ok(bound),
        None => relations.get(StorageBase::Result, path, b),
    }
}
fn local_class(
    module: &TypedModule,
    ty: &Type,
    path: &[PlaceProjection],
    b: &mut Budget,
) -> Check<Option<(Vec<LifeTypeProjection>, u32)>> {
    b.spend(path.len().saturating_add(1))?;
    if path.is_empty() && matches!(ty, Type::Address { .. } | Type::Desc(_)) {
        let life = crate::tasks::frame_shape::declared_life(module, ty, b)?;
        b.spend(life.slots.len())?;
        let mut roots = life.slots.iter().filter(|slot| slot.path.is_empty());
        let Some(root) = roots.next() else {
            return Ok(None);
        };
        if roots.next().is_some() {
            return Err(err("input role repeats its actual root class"));
        }
        return Ok(Some((vec![], root.binder)));
    }
    match ty {
        Type::BorrowedStorage { life, .. } => {
            b.spend(
                life.slots
                    .len()
                    .saturating_mul(path.len().saturating_add(1)),
            )?;
            let mut rows = life.slots.iter().filter(|row| row.path == path);
            let row = rows.next();
            if rows.next().is_some() {
                return Err(err("result field repeats its actual qualified slot"));
            }
            Ok(row.map(|row| (vec![], row.binder)))
        }
        Type::Closure(id) => {
            let Some((PlaceProjection::Field(capture), tail)) = path.split_first() else {
                return Ok(None);
            };
            let definition = module
                .closures
                .get(id.0 as usize)
                .filter(|row| row.id == *id)
                .ok_or_else(|| err("result field closure identity is absent"))?;
            let entry = module
                .functions
                .get(definition.function.0 as usize)
                .filter(|row| row.id == definition.function && row.environment == Some(*id))
                .ok_or_else(|| err("result field closure entry is absent"))?;
            let ty = definition
                .captures
                .get(*capture as usize)
                .ok_or_else(|| err("result field closure capture is absent"))?;
            match declared_bound(module, entry, StorageBase::Capture(*capture), ty, tail, b)? {
                Some(StorageBinder::Fixed(class)) => Ok(Some((vec![], class))),
                Some(StorageBinder::Invocation(_)) => Err(err(
                    "persistent result field uses an invocation intrinsic class",
                )),
                None => Ok(None),
            }
        }
        Type::Address { inner, .. } if !path.is_empty() => {
            let Some((mut child, class)) = local_class(module, inner, path, b)? else {
                return Ok(None);
            };
            b.spend(child.len().saturating_add(1))?;
            child.insert(0, LifeTypeProjection::Pointee);
            Ok(Some((child, class)))
        }
        _ => {
            let Some((step, tail)) = path.split_first() else {
                return Ok(None);
            };
            let child = match (ty, step) {
                (Type::Tuple(fields), PlaceProjection::Field(index)) => fields.get(*index as usize),
                (
                    Type::Record { fields, .. } | Type::Package { fields, .. },
                    PlaceProjection::Field(index),
                ) => fields.get(*index as usize).map(|(_, ty)| ty),
                (Type::Array { element, length }, PlaceProjection::Field(index))
                    if index < length =>
                {
                    Some(element.as_ref())
                }
                (Type::Enum { variants, .. }, PlaceProjection::Variant(index)) => variants
                    .get(index.variant as usize)
                    .and_then(|(_, fields)| fields.get(index.index as usize)),
                (Type::ErrorUnion { error, result }, PlaceProjection::ErrorArm(failed)) => {
                    Some(if *failed {
                        error.as_ref()
                    } else {
                        result.as_ref()
                    })
                }
                _ => None,
            }
            .ok_or_else(|| err("result field relation selects no actual child"))?;
            let Some((mut child, class)) = local_class(module, child, tail, b)? else {
                return Ok(None);
            };
            b.spend(child.len().saturating_add(1))?;
            child.insert(0, LifeTypeProjection::Place(*step));
            Ok(Some((child, class)))
        }
    }
}
fn source_bound(
    module: &TypedModule,
    f: &TypedFunction,
    source: &Source,
    relations: &mut relation_index::Relations<'_>,
    b: &mut Budget,
) -> Check<StorageBinder> {
    if let Some(origin) = &source.retained_port {
        return match origin {
            PortOrigin::Task { .. } => task_address::source_relation(module, f, source, b),
            PortOrigin::Dyn { carrier, port } => {
                let HoldRoot::Parameter(index) = source.base else {
                    return captured_owner::class(module, f, source, b);
                };
                dyn_returns::source_relation(module, f, index, carrier, *port, b)
            }
            PortOrigin::Reference { .. } => reference_cells::relation(module, f, source, b),
        };
    }
    let (base, path) = match source.base {
        HoldRoot::Parameter(index) => (StorageBase::Parameter(index), source.leaf.as_slice()),
        HoldRoot::Capture(index) => (StorageBase::Capture(index), source.leaf.as_slice()),
        _ => return Err(err("result field retains activation storage")),
    };
    let ty = match base {
        StorageBase::Parameter(index) => f
            .parameters
            .get(index as usize)
            .and_then(|local| f.locals.get(local.0 as usize))
            .map(|local| &local.ty),
        StorageBase::Capture(index) => f
            .environment
            .and_then(|id| module.closures.get(id.0 as usize))
            .and_then(|definition| definition.captures.get(index as usize)),
        _ => None,
    }
    .ok_or_else(|| err("result field source has no exact actual input type"))?;
    if let Some(bound) = declared_bound(module, f, base, ty, path, b)? {
        return Ok(bound);
    }
    if matches!(source.base, HoldRoot::Capture(_)) {
        captured_owner::class(module, f, source, b)
    } else {
        relations.get(base, path, b)
    }
}
fn input_declaration<'a>(
    module: &'a TypedModule,
    f: &'a TypedFunction,
    base: HoldRoot,
) -> Check<(StorageBase, &'a Type)> {
    match base {
        HoldRoot::Parameter(index) => f
            .parameters
            .get(index as usize)
            .and_then(|local| f.locals.get(local.0 as usize))
            .map(|local| (StorageBase::Parameter(index), &local.ty)),
        HoldRoot::Capture(index) => f
            .environment
            .and_then(|id| module.closures.get(id.0 as usize))
            .and_then(|definition| definition.captures.get(index as usize))
            .map(|ty| (StorageBase::Capture(index), ty)),
        _ => None,
    }
    .ok_or_else(|| err("input source has no actual declaration"))
}
fn pointee_bound(
    module: &TypedModule,
    f: &TypedFunction,
    base: StorageBase,
    original: &Type,
    path: &[PlaceProjection],
    b: &mut Budget,
) -> Check<Option<StorageBinder>> {
    let Type::Address { inner, .. } = original else {
        return declared_bound(module, f, base, original, path, b);
    };
    let Some((mut child, class)) = local_class(module, inner, path, b)? else {
        return Ok(None);
    };
    b.spend(child.len().saturating_add(1))?;
    child.insert(0, LifeTypeProjection::Pointee);
    let Some(header) = &f.storage_life else {
        return Ok(None);
    };
    b.spend(
        header
            .classes
            .len()
            .saturating_mul(child.len().saturating_add(1)),
    )?;
    let mut rows = header.classes.iter().filter(|row| {
        row.position.base == base && row.position.path == child && row.position.class == class
    });
    let row = rows.next();
    if rows.next().is_some() {
        return Err(err("pointee source repeats its complete class occurrence"));
    }
    Ok(row.map(|row| row.binder))
}
fn description_source_bound(
    module: &TypedModule,
    f: &TypedFunction,
    source: &Source,
    relations: &mut relation_index::Relations<'_>,
    b: &mut Budget,
) -> Check<StorageBinder> {
    if source.retained_port.is_some()
        || source.heap_element.is_some()
        || !source.heap_parents.is_empty()
    {
        return source_bound(module, f, source, relations, b);
    }
    let (base, original) = input_declaration(module, f, source.base)?;
    b.spend(source.leaf.len().saturating_add(source.inline.len()))?;
    let mut path = source.leaf.clone();
    path.extend(&source.inline);
    if let Type::Address { inner, .. } = original {
        let target = call_results::destination(module, inner, &path, b)?;
        if matches!(target, Type::Desc(_)) {
            return pointee_bound(module, f, base, original, &path, b)?.ok_or_else(|| {
                err("description pointee lacks its complete original class occurrence")
            });
        }
    }
    if let Some(bound) = declared_bound(module, f, base, original, &path, b)? {
        return Ok(bound);
    }
    source_bound(module, f, source, relations, b)
}
fn check_symbolic(
    module: &TypedModule,
    f: &TypedFunction,
    path: &[PlaceProjection],
    permission: &SymbolicPermission,
    head: bool,
    description: bool,
    summary: &Summary,
    relations: &mut relation_index::Relations<'_>,
    b: &mut Budget,
) -> Check<()> {
    dyn_returns::check_parent_escape(f, &permission.parents, b)?;
    if permission.receiver {
        let header = f
            .storage_life
            .as_ref()
            .ok_or_else(|| err("result field invocation parent lacks its actual relation"))?;
        let expected = signature_life::outer(header, StorageBase::Receiver)
            .ok_or_else(|| err("result field invocation parent omits Receiver"))?;
        let actual = match declared_bound(module, f, StorageBase::Result, &f.return_type, path, b)?
        {
            Some(bound) => bound,
            None => relations.get(StorageBase::Result, path, b)?,
        };
        if actual != expected {
            return Err(err("result field escaped its actual invocation parent"));
        }
    }
    for source in &permission.sources {
        source_work(std::slice::from_ref(source), b)?;
        match source.base {
            HoldRoot::Static => {
                if !source.leaf.is_empty()
                    || !source.inline.is_empty()
                    || source.retained_port.is_some()
                    || source.heap_element.is_some()
                    || !source.heap_parents.is_empty()
                {
                    return Err(err("static result field has an unproven projection"));
                }
                continue;
            }
            HoldRoot::Local(_) => return Err(err("result field retains activation-local storage")),
            HoldRoot::Capture(_) if f.environment.is_none() => {
                return Err(err("result field invents an external capture source"));
            }
            HoldRoot::Parameter(index) => {
                if let Some(upper) = &f.reference_return_bound {
                    b.spend(upper.len())?;
                    if !source.leaf.is_empty() || upper.binary_search(&index).is_err() {
                        return Err(err(
                            "result field exceeds its declared direct reference upper",
                        ));
                    }
                }
            }
            HoldRoot::Capture(_) => {}
        }
        if let Some(contract) = &f.callable_return_contract {
            if head {
                let target =
                    declared_bound(module, f, StorageBase::Result, &f.return_type, path, b)?;
                if let Some(target) = target {
                    if target
                        != if description {
                            description_source_bound(module, f, source, relations, b)?
                        } else {
                            source_bound(module, f, source, relations, b)?
                        }
                    {
                        return Err(err(
                            "returned environment head changes its complete lifetime relation",
                        ));
                    }
                } else if !crate::dynamic_witness::semantic_patterns::checked_result_boundary(
                    module, f, b,
                )? {
                    return Err(err(
                        "returned environment head lacks its complete original result proof",
                    ));
                }
                continue;
            }
            if returned_capture_source(module, f, contract, path, source, b)? {
                continue;
            }
            return Err(err(
                "result field source exceeds the complete returned environment upper",
            ));
        }
        if f.storage_life.is_none() {
            return Err(err(
                "borrowed result field lacks its complete declared relation",
            ));
        }
        let target = match declared_bound(module, f, StorageBase::Result, &f.return_type, path, b)?
        {
            Some(bound) => bound,
            None => {
                // A view's extra real head is an ancestor, never a new data target.
                // When there is no physical head binder the exact original-world
                // Return proof still has to establish the full output boundary.
                if head
                    && crate::dynamic_witness::semantic_patterns::checked_result_boundary(
                        module, f, b,
                    )?
                {
                    continue;
                }
                relations.get(StorageBase::Result, path, b)?
            }
        };
        b.spend(
            summary
                .receiver_results
                .len()
                .saturating_mul(path.len().saturating_add(1)),
        )?;
        if permission.receiver
            || summary
                .receiver_results
                .iter()
                .any(|row| row.as_slice() == path)
        {
            continue;
        }
        if target
            != if description {
                description_source_bound(module, f, source, relations, b)?
            } else {
                source_bound(module, f, source, relations, b)?
            }
        {
            return Err(err(
                "result field changes its complete original lifetime relation",
            ));
        }
    }
    Ok(())
}
pub(in crate::stored_flow) fn validate_results(
    module: &TypedModule,
    f: &TypedFunction,
    summary: &Summary,
    solution: &Solution,
    graph: &FunctionGraph,
    relations: &mut relation_index::Relations<'_>,
    b: &mut Budget,
) -> Check<()> {
    validate_returns(module, f, solution, graph, summary, b)?;
    for (path, row) in &summary.field_result_facts {
        let ty = call_results::destination(module, &f.return_type, path, b)?;
        if nonempty(&row.description) && !matches!(ty, Type::Desc(_)) {
            return Err(err(
                "description result facts select another actual field kind",
            ));
        }
        check_symbolic(
            module,
            f,
            path,
            &row.address,
            matches!(ty, Type::Dyn { .. } | Type::OwnedDyn { .. }),
            matches!(ty, Type::Desc(_)),
            summary,
            relations,
            b,
        )?;
        check_symbolic(
            module,
            f,
            path,
            &row.description,
            false,
            true,
            summary,
            relations,
            b,
        )?;
        for (table, head) in &row.views {
            if !matches!(ty,Type::Address{inner,..}if matches!(inner.as_ref(),Type::Dyn{..}|Type::OwnedDyn{..}))
                && !matches!(ty, Type::Dyn { .. } | Type::OwnedDyn { .. })
            {
                return Err(err(
                    "dynamic view result facts select another actual field kind",
                ));
            }
            if let Some(table) = table {
                b.spend(module.dyn_vtables.len())?;
                if !module.dyn_vtables.iter().any(|row| row.id == *table) {
                    return Err(err("result field dynamic table is absent"));
                }
            }
            check_symbolic(module, f, path, head, true, false, summary, relations, b)?;
        }
    }
    Ok(())
}

fn required_return_known(
    module: &TypedModule,
    ty: &Type,
    payload: &Payload,
    root: &FieldFacts,
    b: &mut Budget,
) -> Check<bool> {
    let whole = ty;
    let mut pending = vec![(ty, Path::new(), 0usize)];
    let mut known = true;
    while let Some((ty, path, depth)) = pending.pop() {
        b.spend(path.len().saturating_add(1))?;
        if depth > b.limits.max_call_depth.min(128) {
            return Err(limit("result field directory depth exceeded"));
        }
        if !call_results::path_active(module, whole, payload, &path, b)? {
            continue;
        }
        let current = actual(payload, &path, root, b)?;
        if matches!(ty, Type::Desc(_))
            && current.is_none_or(|facts| {
                facts.address.sources.is_empty() && facts.description.sources.is_empty()
            })
        {
            let physical = if path.is_empty() {
                &payload.address
            } else {
                match leaf(payload, &path, b)? {
                    Some(permission) => permission,
                    None => {
                        known = false;
                        continue;
                    }
                }
            };
            if physical.sources.is_empty() {
                known = false;
            }
        }
        if option_address::signature(ty) && current.is_none_or(|facts| facts.option.mask() == 0) {
            known = false;
        }
        if current.is_some_and(|facts| facts.creation == Some(false)) {
            known = false;
        }
        let mut storage = ty;
        while let Type::BorrowedStorage { storage: inner, .. } = storage {
            storage = inner;
        }
        let mut fields = Vec::new();
        match storage {
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
                fields.extend((0..*length).map(|i| (PlaceProjection::Field(i), element.as_ref())));
            }
            Type::Closure(id) => {
                let definition = module
                    .closures
                    .get(id.0 as usize)
                    .filter(|row| row.id == *id)
                    .ok_or_else(|| err("result field closure identity is absent"))?;
                b.spend(definition.captures.len())?;
                fields.extend(
                    definition
                        .captures
                        .iter()
                        .enumerate()
                        .map(|(i, t)| (PlaceProjection::Field(i as u32), t)),
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
    Ok(known)
}
fn required_return(
    module: &TypedModule,
    ty: &Type,
    payload: &Payload,
    root: &FieldFacts,
    b: &mut Budget,
) -> Check<()> {
    if !required_return_known(module, ty, payload, root, b)? {
        return Err(err(
            "actual Return omits a required initialized field channel",
        ));
    }
    Ok(())
}
fn returned_capture_source(
    module: &TypedModule,
    f: &TypedFunction,
    contract: &CallableReturnContract,
    path: &[PlaceProjection],
    source: &Source,
    b: &mut Budget,
) -> Check<bool> {
    let leaves =
        crate::retained::sources::capture_leaves(module, contract.closure, &b.limits.clone(), b)?;
    for leaf in leaves {
        let StorageBase::Capture(index) = leaf.position.base else {
            continue;
        };
        b.spend(leaf.position.path.len().saturating_add(1))?;
        let mut physical = vec![PlaceProjection::Field(index)];
        for step in &leaf.position.path {
            match step {
                ContractProjection::Place(step) => physical.push(*step),
                ContractProjection::Capture(index) => physical.push(PlaceProjection::Field(*index)),
                _ => return Ok(false),
            }
        }
        if physical != path {
            continue;
        }
        let Some(bindings) = &contract.capture_sources else {
            return Ok(false);
        };
        b.spend(
            bindings
                .len()
                .saturating_mul(leaf.position.path.len().saturating_add(1)),
        )?;
        let Some(binding) = bindings
            .iter()
            .find(|binding| binding.position == leaf.position)
        else {
            return Ok(false);
        };
        if source.base == HoldRoot::Static {
            b.spend(binding.sources.len())?;
            for atom in &binding.sources {
                if let StorageSource::Static {
                    leaf: expected,
                    mode,
                } = atom
                {
                    type_work(expected, &b.limits.clone(), b)?;
                    type_work(leaf.ty, &b.limits.clone(), b)?;
                    if *mode == StoredBorrowMode::SharedAddress && expected == leaf.ty {
                        return Ok(true);
                    }
                }
            }
            return Ok(false);
        }
        let HoldRoot::Parameter(_) = source.base else {
            return Ok(false);
        };
        let actual = returned_environment::actual_input_position(module, f, source, b)?;
        b.spend(
            binding
                .sources
                .len()
                .saturating_mul(actual.path.len().saturating_add(1)),
        )?;
        return Ok(binding
            .sources
            .iter()
            .any(|atom| matches!(atom,StorageSource::Exact(position)if position==&actual)));
    }
    Ok(false)
}

pub(super) fn snapshot_rows(
    module: &TypedModule,
    f: &TypedFunction,
    ty: &Type,
    payload: &Payload,
    solution: &Solution,
    root: bool,
    b: &mut Budget,
) -> Check<(Vec<(Path, ResultFacts)>, bool)> {
    let mut summary = Summary::default();
    let facts = if root {
        root_facts(ty, payload, b)?
    } else {
        FieldFacts::default()
    };
    let known = required_return_known(module, ty, payload, &facts, b)?;
    if root {
        record(&[], &facts, f, solution, &mut summary, b)?;
    }
    for (path, facts) in &payload.field_facts {
        if !call_results::path_active(module, ty, payload, path, b)? {
            continue;
        }
        record(path, facts, f, solution, &mut summary, b)?;
    }
    Ok((summary.field_result_facts, known))
}
pub(super) fn join_result(to: &mut ResultFacts, from: &ResultFacts, b: &mut Budget) -> Check<()> {
    join_symbolic(&mut to.address, &from.address, b)?;
    join_symbolic(&mut to.description, &from.description, b)?;
    for view in &from.views {
        result_work(to, b)?;
        source_work(&view.1.sources, b)?;
        source_work(&view.1.parents, b)?;
        if !to.views.contains(view) {
            b.spend(1)?;
            to.views.push(view.clone());
        }
    }
    to.option.join(&from.option, b)?;
    if let Some(creation) = from.creation {
        to.creation = Some(to.creation.unwrap_or(false) || creation);
    }
    Ok(())
}
pub(super) fn validate_after_rows(
    module: &TypedModule,
    f: &TypedFunction,
    base: HoldRoot,
    through: bool,
    ty: &Type,
    rows: &[(Path, ResultFacts)],
    b: &mut Budget,
) -> Check<()> {
    let original = match base {
        HoldRoot::Parameter(index) => f
            .parameters
            .get(index as usize)
            .and_then(|local| f.locals.get(local.0 as usize))
            .map(|local| &local.ty),
        HoldRoot::Capture(index) => f
            .environment
            .and_then(|id| module.closures.get(id.0 as usize))
            .and_then(|d| d.captures.get(index as usize)),
        _ => None,
    }
    .ok_or_else(|| err("auxiliary after-state has no actual input declaration"))?;
    let storage = match base {
        HoldRoot::Parameter(index) => StorageBase::Parameter(index),
        HoldRoot::Capture(index) => StorageBase::Capture(index),
        _ => unreachable!(),
    };
    let mut relations = relation_index::Relations::new(f, b)?;
    for (path, row) in rows {
        let destination = call_results::destination(module, ty, path, b)?;
        if !row.description.sources.is_empty() && !matches!(destination, Type::Desc(_)) {
            return Err(err(
                "description after-state selects another actual field kind",
            ));
        }

        for (permission, description) in
            std::iter::once((&row.address, matches!(destination, Type::Desc(_))))
                .chain(std::iter::once((&row.description, true)))
                .chain(row.views.iter().map(|(_, head)| (head, false)))
        {
            dyn_returns::check_parent_escape(f, &permission.parents, b)?;
            if permission.receiver {
                return Err(err(
                    "mutable after-state retains the retired invocation head",
                ));
            }
            for source in &permission.sources {
                source_work(std::slice::from_ref(source), b)?;
                if source.base == HoldRoot::Static {
                    if !source.leaf.is_empty()
                        || !source.inline.is_empty()
                        || source.retained_port.is_some()
                        || source.heap_element.is_some()
                        || !source.heap_parents.is_empty()
                    {
                        return Err(err(
                            "auxiliary after-state invents a projected Static source",
                        ));
                    }
                    continue;
                }
                if matches!(source.base, HoldRoot::Local(_))
                    || f.environment.is_none() && matches!(source.base, HoldRoot::Capture(_))
                {
                    return Err(err("auxiliary after-state retains activation-local data"));
                }
                let target = if through {
                    pointee_bound(module, f, storage, original, path, b)?
                } else {
                    declared_bound(module, f, storage, original, path, b)?
                };
                if let Some(bound) = target {
                    let actual = if description {
                        description_source_bound(module, f, source, &mut relations, b)?
                    } else {
                        source_bound(module, f, source, &mut relations, b)?
                    };
                    if actual != bound {
                        return Err(err(
                            "auxiliary after-state changes its complete declared lifetime relation",
                        ));
                    }
                } else if source.base != base
                    || source.leaf != *path
                    || !source.inline.is_empty()
                    || source.retained_port.is_some()
                    || source.heap_element.is_some()
                    || !source.heap_parents.is_empty()
                {
                    return Err(err(
                        "auxiliary after-state lacks its exact persistent input boundary",
                    ));
                }
            }
        }
    }
    Ok(())
}

pub(in crate::stored_flow) fn required_channels_known(
    module: &TypedModule,
    ty: &Type,
    payload: &Payload,
    b: &mut Budget,
) -> Check<bool> {
    let root = root_facts(ty, payload, b)?;
    let mut known = required_return_known(module, ty, payload, &root, b)?;
    for path in call_results::physical_slots(module, ty, b)? {
        if !call_results::path_active(module, ty, payload, &path, b)? {
            continue;
        }
        let actual = if path.is_empty() {
            Some(&payload.address)
        } else {
            leaf(payload, &path, b)?
        };
        known &= actual.map_or(false, |permission| !permission.sources.is_empty());
    }
    Ok(known)
}

#[cfg(test)]
mod pointee_scope_tests {
    use super::*;
    #[test]
    fn description_after_uses_pointee_class_not_the_outer_pointer_window() {
        let (mut module, _) = crate::tests::baseline();
        let mut f = module.functions[0].clone();
        let description = Type::Desc(Box::new(Type::I32));
        f.parameters = vec![LocalId(0), LocalId(1)];
        f.locals = vec![
            LocalDecl {
                id: LocalId(0),
                name: "slot".into(),
                ty: Type::Address {
                    mutable: true,
                    inner: Box::new(description.clone()),
                },
                mutable: false,
                span: f.span,
            },
            LocalDecl {
                id: LocalId(1),
                name: "other".into(),
                ty: description.clone(),
                mutable: false,
                span: f.span,
            },
        ];
        f.storage_life = Some(StorageLifeContract {
            fixed_binder_count: 0,
            invocation_binder_count: 2,
            bindings: vec![
                StorageLifeBinding {
                    position: StoragePosition {
                        base: StorageBase::Parameter(0),
                        path: vec![],
                    },
                    binder: StorageBinder::Invocation(0),
                },
                StorageLifeBinding {
                    position: StoragePosition {
                        base: StorageBase::Parameter(1),
                        path: vec![],
                    },
                    binder: StorageBinder::Invocation(0),
                },
            ],
            classes: vec![SignatureLifeClassBinding {
                position: SignatureLifeClass {
                    base: StorageBase::Parameter(0),
                    path: vec![LifeTypeProjection::Pointee],
                    class: 0,
                },
                binder: StorageBinder::Invocation(1),
            }],
            static_bindings: vec![],
        });
        module.functions[0] = f.clone();
        let source = |index| Source {
            base: HoldRoot::Parameter(index),
            leaf: vec![],
            inline: vec![],
            retained_port: None,
            heap_parents: vec![],
            heap_element: None,
            static_dependency: StaticProvenance::NotStatic,
        };
        let limits = Limits::default();
        let mut b = Budget::new(&limits);
        let mut row = ResultFacts {
            description: SymbolicPermission {
                sources: vec![source(0)],
                ..Default::default()
            },
            ..Default::default()
        };
        validate_after_rows(
            &module,
            &f,
            HoldRoot::Parameter(0),
            true,
            &description,
            &[(vec![], row.clone())],
            &mut b,
        )
        .unwrap();
        row.description.sources = vec![source(1)];
        assert!(
            validate_after_rows(
                &module,
                &f,
                HoldRoot::Parameter(0),
                true,
                &description,
                &[(vec![], row)],
                &mut b
            )
            .is_err(),
            "another descriptor sharing only alpha cannot replace beta's distinct class"
        );
    }
}
