//! Current physical pointee values. An address SSA is a location/parent loan,
//! never a cached copy of mutable aggregate contents.
use super::*;

pub(super) fn aggregate(ty: &Type) -> bool {
    matches!(
        ty,
        Type::BorrowedStorage { .. }
            | Type::Tuple(_)
            | Type::Record { .. }
            | Type::Package { .. }
            | Type::Array { .. }
            | Type::Enum { .. }
            | Type::ErrorUnion { .. }
    )
}
/// A reference to owner-only data still needs its actual root and parent loan.
/// Only shapes that carry stored permissions or tracked sum state need a
/// second lookup of the referent's dynamic permission tree.
pub(super) fn stored_aggregate(ty: &Type, b: &mut Budget) -> CompileResult<bool> {
    Ok(aggregate(ty) && needs_state(ty, b, 0)?)
}
pub(super) fn context(ty: &Type) -> bool {
    crate::stored_callables::mutable_context_pointee(ty)
}
pub(super) fn rows(ty: &Type, b: &mut Budget) -> CompileResult<Vec<(Path, StoredBorrowMode)>> {
    let mut out = Vec::new();
    compound_slot_rows(ty, &mut Vec::new(), &mut out, b)?;
    Ok(out)
}
fn top_permissions(value: &Value) -> impl Iterator<Item = &Permission> {
    std::iter::once(&value.pointer)
        .chain(std::iter::once(&value.held))
        .chain(std::iter::once(&value.store_description))
        .chain(value.slots.values())
        .chain(value.environment.values())
        .chain(value.task_ports.values())
        .chain(value.dyn_ports.values())
        .chain(
            value
                .projections
                .values()
                .flat_map(|payload| [&payload.held, &payload.store_description]),
        )
}
pub(super) fn permissions(value: &Value) -> impl Iterator<Item = &Permission> {
    std::iter::once(value)
        .chain(value.pointees.values())
        .flat_map(top_permissions)
}
pub(super) fn targets(value: &Value) -> impl Iterator<Item = &Target> {
    permissions(value).flat_map(|permission| permission.targets.iter())
}
/// Copies only summary identities; local loan numbers never cross an arena.
pub(super) fn summary(value: &Value, b: &mut Budget) -> CompileResult<Value> {
    let mut out = snapshot(value, b)?;
    fn clear(value: &mut Value) {
        value.pointer.loans.clear();
        value.held.loans.clear();
        value.store_description.loans.clear();
        for p in value.projections.values_mut() {
            p.held.loans.clear();
            p.store_description.loans.clear();
        }
        for p in value
            .slots
            .values_mut()
            .chain(value.environment.values_mut())
            .chain(value.task_ports.values_mut())
            .chain(value.dyn_ports.values_mut())
        {
            p.loans.clear();
        }
    }
    clear(&mut out);
    for value in out.pointees.values_mut() {
        clear(value);
    }
    Ok(out)
}
pub(super) fn project(
    value: &Value,
    path: &[PlaceProjection],
    ty: Option<&Type>,
    b: &mut Budget,
) -> CompileResult<Value> {
    if path.is_empty() {
        return snapshot(value, b);
    }
    let logical: Vec<_> = path
        .iter()
        .copied()
        .map(ContractProjection::Place)
        .collect();
    fee(b, path.len().saturating_add(1))?;
    let mut out = Value {
        pointees: project_pointees(&value.pointees, path, b)?,
        saved_heads: project_heads(&value.saved_heads, path, b)?,
        slots: environment_memory::project(&value.slots, path, b)?,
        environment: environment_memory::project(&value.environment, path, b)?,
        task_ports: environment_memory::project(&value.task_ports, &logical, b)?,
        dyn_ports: environment_memory::project(&value.dyn_ports, &logical, b)?,
        enum_variants: value.enum_variants.project(path, b)?,
        ..Value::default()
    };
    if matches!(ty, Some(Type::Address { .. })) {
        if let Some(pointer) = out.slots.remove(&Vec::new()) {
            out.pointer = pointer;
        }
    }
    for p in out.environment.values() {
        union(&mut out.held, p, b)?;
    }
    // Whole metadata belongs to this root only. A field receives only its
    // explicitly located payload; parent creation/description/upper state is
    // not a property of every child.
    if let Some(payload) = value.projections.get(path) {
        install_root(&mut out, copy_projection(payload, b)?, b)?;
    }
    for (full, payload) in &value.projections {
        fee(b, full.len().saturating_add(path.len()).saturating_add(1))?;
        if let Some(tail) = full.strip_prefix(path)
            && !tail.is_empty()
        {
            union(&mut out.held, &payload.held, b)?;
            out.projections
                .insert(tail.to_vec(), copy_projection(payload, b)?);
        }
    }
    Ok(out)
}
fn captured_value<'a>(
    state: &'a State,
    index: u32,
    b: &mut Budget,
) -> CompileResult<Option<&'a Value>> {
    if let Some(local) = captured_view_local(state, HoldRoot::Capture(index), b)? {
        return Ok(state.locals.get(&local));
    }
    Ok(state.captures.get(&index))
}
fn root_value<'a>(
    state: &'a State,
    root: &Target,
    b: &mut Budget,
) -> CompileResult<Option<&'a Value>> {
    if root.retained_port.is_some() || root.heap_element.is_some() || !root.heap_parents.is_empty()
    {
        return Err(failure("ordinary pointee crosses an opaque/heap boundary"));
    }
    ordered(
        b,
        state
            .external
            .len()
            .saturating_add(state.locals.len())
            .saturating_add(state.captures.len()),
        0,
    )?;
    if !root.stored.is_empty() {
        return Ok(state.external.get(&root.base));
    }
    Ok(match root.base {
        HoldRoot::Local(local) => state.locals.get(&local),
        HoldRoot::Parameter(_) => state.external.get(&root.base),
        HoldRoot::Capture(index) => {
            let value = captured_value(state, index, b)?;
            if value.is_none_or(|value| !value.pointer.targets.is_empty()) {
                state.external.get(&root.base)
            } else {
                value
            }
        }
        HoldRoot::Static => None,
    })
}
fn candidate(
    state: &State,
    root: &Target,
    ty: Option<&Type>,
    b: &mut Budget,
) -> CompileResult<Option<Value>> {
    let Some(value) = root_value(state, root, b)? else {
        return Ok(None);
    };
    if root.stored.is_empty() {
        return project(value, &root.place, ty, b).map(Some);
    }
    // Select a saved head, then a distinct projection inside its referent.
    ordered(b, value.pointees.len(), root.stored.len())?;
    let Some(referent) = value.pointees.get(&root.stored) else {
        return Ok(None);
    };
    let out = project(referent, &root.place, ty, b)?;
    Ok(Some(out))
}

fn inline_store_data(root: &Target, ty: &Type, b: &mut Budget) -> CompileResult<bool> {
    if root.retained_port.is_some() || root.heap_element.is_none() || !ty.is_copy_value() {
        return Ok(false);
    }
    // This target is established by the checked Store window and carried
    // through its real field projections. The ordinary access check still
    // uses that same manager, element path and live parent loans.
    crate::pod_store::inline_data(ty, b)?;
    Ok(true)
}

pub(super) fn complete(value: &Value, ty: &Type, b: &mut Budget) -> CompileResult<bool> {
    if needs_state(ty, b, 0)? && !value.enum_variants.complete_tree(ty, b)? {
        return Ok(false);
    }
    if value.creation_complete == Some(false)
        || value
            .projections
            .values()
            .any(|p| p.creation_complete == Some(false))
    {
        return Ok(false);
    }
    for (path, _) in rows(ty, b)? {
        if !value.enum_variants.slot_active(&path) {
            continue;
        }
        ordered(b, value.slots.len(), path.len())?;
        if value.slots.get(&path).is_none_or(|p| p.targets.is_empty()) {
            return Ok(false);
        }
    }
    Ok(true)
}
/// During inference, retain an unresolved creator obligation until every
/// actual pointer candidate has its complete current aggregate fact.
pub(super) fn all_candidates_complete(
    state: &State,
    address: &Permission,
    ty: &Type,
    b: &mut Budget,
) -> CompileResult<bool> {
    if address.targets.is_empty() {
        return Ok(false);
    }
    for root in &address.targets {
        fee(b, target_width(root).saturating_add(1))?;
        let Some(value) = candidate(state, root, Some(ty), b)? else {
            return Ok(false);
        };
        if !complete(&value, ty, b)? {
            return Ok(false);
        }
    }
    Ok(true)
}
pub(super) fn read(
    state: &State,
    address: &Permission,
    ty: Option<&Type>,
    check: bool,
    b: &mut Budget,
) -> CompileResult<Value> {
    if check && address.targets.is_empty() {
        return Err(failure("aggregate pointee has no actual address candidate"));
    }
    let mut out = Value::default();
    for root in &address.targets {
        fee(b, target_width(root).saturating_add(1))?;
        if let Some(ty) = ty && inline_store_data(root, ty, b)? {
            continue;
        }
        let Some(value) = candidate(state, root, ty, b)? else {
            if check {
                return Err(failure(
                    "aggregate pointee has an unresolved actual address candidate",
                ));
            }
            continue;
        };
        if check {
            ordered(b, state.vacant.len(), target_width(root))?;
            if state.vacant.iter().any(|hole| {
                hole.base == root.base
                    && hole.stored == root.stored
                    && (hole.place.starts_with(&root.place) || root.place.starts_with(&hole.place))
            }) {
                return Err(failure("aggregate pointee crosses vacant physical storage"));
            }
            if let Some(ty) = ty
                && !complete(&value, ty, b)?
            {
                return Err(failure(
                    "aggregate pointee candidate omits an active physical source or sum state",
                ));
            }
        }
        merge(&mut out, &value, b)?;
    }
    Ok(out)
}
/// A stored input target is the occupant at call entry. A missing target
/// discharges a separation pair only if its exact physical slot cannot be
/// active in any complete incoming value.
pub(super) fn inactive_input_slot(
    callee: &TypedFunction,
    source: &Target,
    arguments: &[ValueId],
    values: &[Value],
    state: &State,
    b: &mut Budget,
) -> CompileResult<bool> {
    let HoldRoot::Parameter(index) = source.base else { return Ok(false) };
    if source.retained_port.is_some() || source.stored.is_empty()
        || source.heap_element.is_some() || !source.heap_parents.is_empty()
    {
        return Ok(false);
    }
    let index = index as usize;
    let (Some(local), Some(actual)) = (callee.parameters.get(index), arguments.get(index)) else {
        return Err(failure("separation selects an absent input"));
    };
    let ty = &callee.locals[local.0 as usize].ty;
    let value = &values[actual.0 as usize];
    let (inner, from_memory) = match ty {
        Type::Address { inner, .. } => (inner.as_ref(), true),
        other => (other, false),
    };
    if !aggregate(inner) { return Ok(false); }
    let mut declared = false;
    for (path, _) in rows(inner, b)? {
        fee(b, path.len().saturating_add(source.stored.len()).saturating_add(1))?;
        if path == source.stored {
            declared = true;
            break;
        }
    }
    if !declared { return Ok(false); }
    let memory;
    let current = if from_memory {
        memory = read(state, &value.pointer, Some(inner), true, b)?;
        &memory
    } else {
        value
    };
    Ok(current.enum_variants.present() && complete(current, inner, b)?
        && !current.enum_variants.slot_active(&source.stored))
}

fn replace(
    value: &mut Value,
    path: &[PlaceProjection],
    incoming: &Value,
    ty: &Type,
    strong: bool,
    b: &mut Budget,
) -> CompileResult<()> {
    if path.is_empty() {
        if strong {
            let previous = copy_pointees(&value.pointees, b)?;
            *value = snapshot(incoming, b)?;
            value.pointees = previous;
        } else {
            merge(value, incoming, b)?;
        }
        return Ok(());
    }
    let mut slots = copy_permission_rows(&incoming.slots, b)?;
    if matches!(ty, Type::Address { .. }) {
        slots.insert(Vec::new(), copy_permission(&incoming.pointer, b)?);
    }
    fee(b, path.len().saturating_add(1))?;
    let logical: Vec<_> = path
        .iter()
        .copied()
        .map(ContractProjection::Place)
        .collect();
    if strong {
        environment_memory::replace(&mut value.slots, path, slots, b)?;
        environment_memory::replace(
            &mut value.environment,
            path,
            copy_permission_rows(&incoming.environment, b)?,
            b,
        )?;
        environment_memory::replace(
            &mut value.task_ports,
            &logical,
            copy_permission_rows(&incoming.task_ports, b)?,
            b,
        )?;
        environment_memory::replace(
            &mut value.dyn_ports,
            &logical,
            copy_permission_rows(&incoming.dyn_ports, b)?,
            b,
        )?;
    } else {
        for (tail, p) in slots {
            fee(b, path.len().saturating_add(tail.len()))?;
            let mut full = path.to_vec();
            full.extend(tail);
            ordered(b, value.slots.len(), full.len())?;
            union(value.slots.entry(full).or_default(), &p, b)?;
        }
        merge_rows(&mut value.environment, path, &incoming.environment, b)?;
        merge_rows(&mut value.task_ports, &logical, &incoming.task_ports, b)?;
        merge_rows(&mut value.dyn_ports, &logical, &incoming.dyn_ports, b)?;
    }
    install_projection(value, path, incoming, strong, b)?;
    value
        .enum_variants
        .replace(path, &incoming.enum_variants, strong, b)?;
    recompute_held(value, b)?;
    Ok(())
}
pub(super) fn write(
    state: &mut State,
    address: &Permission,
    incoming: &Value,
    ty: &Type,
    strong: bool,
    b: &mut Budget,
) -> CompileResult<()> {
    for root in &address.targets {
        fee(b, target_width(root).saturating_add(1))?;
        if inline_store_data(root, ty, b)? {
            fee(b, 1)?;
            if *incoming != Value::default() {
                return Err(failure("plain Store element write carries auxiliary stored state"));
            }
            continue;
        }
        if root.retained_port.is_some()
            || root.heap_element.is_some()
            || !root.heap_parents.is_empty()
        {
            return Err(failure(
                "aggregate replacement crosses an opaque/heap boundary",
            ));
        }
        let view = captured_view_local(state, root.base, b)?;
        let external = !root.stored.is_empty()
            || match root.base {
                HoldRoot::Parameter(_) => true,
                HoldRoot::Capture(index) => state
                    .captures
                    .get(&index)
                    .is_none_or(|value| !value.pointer.targets.is_empty()),
                _ => false,
            };
        let value = if external {
            state.external.get_mut(&root.base)
        } else {
            match root.base {
                HoldRoot::Local(local) => state.locals.get_mut(&local),
                HoldRoot::Capture(index) => {
                    if let Some(local) = view {
                        state.locals.get_mut(&local)
                    } else {
                        state.captures.get_mut(&index)
                    }
                }
                _ => None,
            }
        }
        .ok_or_else(|| {
            failure("aggregate replacement has an unresolved actual destination candidate")
        })?;
        if root.stored.is_empty() {
            let directory = std::mem::take(&mut value.pointees);
            replace(value, &root.place, incoming, ty, strong, b)?;
            value.pointees = directory;
        } else {
            ordered(b, value.pointees.len(), root.stored.len())?;
            let referent = value.pointees.get_mut(&root.stored).ok_or_else(|| {
                failure("aggregate replacement has no current saved-head referent")
            })?;
            replace(referent, &root.place, incoming, ty, strong, b)?;
        }
        if strong {
            for hole in &state.vacant {
                fee(
                    b,
                    target_width(hole)
                        .saturating_add(target_width(root))
                        .saturating_add(1),
                )?;
            }
            state.vacant.retain(|hole| {
                !(hole.base == root.base
                    && hole.stored == root.stored
                    && hole.place.starts_with(&root.place))
            });
        }
    }
    b.stored_slot_updates = true;
    Ok(())
}

fn transport_top<F>(value: &Value, b: &mut Budget, mut source: F) -> CompileResult<Value>
where
    F: FnMut(&Target, &mut Budget) -> CompileResult<Permission>,
{
    fn permission<F>(p: &Permission, b: &mut Budget, source: &mut F) -> CompileResult<Permission>
    where
        F: FnMut(&Target, &mut Budget) -> CompileResult<Permission>,
    {
        let mut out = Permission::default();
        for target in &p.targets {
            union(&mut out, &source(target, b)?, b)?;
        }
        Ok(out)
    }
    fn map<P: Copy + Ord, F>(
        rows: &BTreeMap<Vec<P>, Permission>,
        b: &mut Budget,
        source: &mut F,
    ) -> CompileResult<BTreeMap<Vec<P>, Permission>>
    where
        F: FnMut(&Target, &mut Budget) -> CompileResult<Permission>,
    {
        let mut out = BTreeMap::new();
        for (path, p) in rows {
            fee(b, path.len().saturating_add(1))?;
            out.insert(path.clone(), permission(p, b, source)?);
        }
        Ok(out)
    }
    value.enum_variants.work(b)?;
    value.option_variants.work(b)?;
    let mut origins = |targets: &Targets, b: &mut Budget| -> CompileResult<Targets> {
        let mut out = Targets::new();
        for target in targets {
            for actual in source(target, b)?.targets {
                ordered(b, out.len(), target_width(&actual))?;
                out.insert(actual);
            }
        }
        Ok(out)
    };
    let upper = origins(&value.source_upper, b)?;
    let pointee_holds = origins(&value.pointee_holds, b)?;
    let mut projections = BTreeMap::new();
    for (path, payload) in &value.projections {
        fee(b, path.len().saturating_add(1))?;
        let mut upper = Targets::new();
        for target in &payload.source_upper {
            for target in source(target, b)?.targets {
                ordered(b, upper.len(), target_width(&target))?;
                upper.insert(target);
            }
        }
        payload.option_variants.work(b)?;
        projections.insert(
            path.clone(),
            Projection {
                held: permission(&payload.held, b, &mut source)?,
                source_upper: upper,
                store_description: permission(&payload.store_description, b, &mut source)?,
                option_variants: payload.option_variants.clone(),
                creation_complete: payload.creation_complete,
            },
        );
    }
    Ok(Value {
        pointees: BTreeMap::new(),
        saved_heads: copy_heads(&value.saved_heads, b)?,
        projections,
        pointer: permission(&value.pointer, b, &mut source)?,
        held: permission(&value.held, b, &mut source)?,
        store_description: permission(&value.store_description, b, &mut source)?,
        slots: map(&value.slots, b, &mut source)?,
        environment: map(&value.environment, b, &mut source)?,
        task_ports: map(&value.task_ports, b, &mut source)?,
        dyn_ports: map(&value.dyn_ports, b, &mut source)?,
        enum_variants: value.enum_variants.clone(),
        option_variants: value.option_variants.clone(),
        source_upper: upper,
        pointee_holds,
        creation_complete: value.creation_complete,
    })
}
pub(super) fn check_inputs(
    module: &TypedModule,
    callee: &TypedFunction,
    arguments: &[ValueId],
    values: &[Value],
    state: &State,
    b: &mut Budget,
) -> CompileResult<()> {
    for (local, argument) in callee.parameters.iter().zip(arguments) {
        capture_after::check_value(
            module,
            &callee.locals[local.0 as usize].ty,
            &values[argument.0 as usize],
            state,
            b,
        )?;
    }
    Ok(())
}

pub(super) fn sum_transfer(
    f: &TypedFunction,
    i: &Instruction,
    values: &[Value],
    b: &mut Budget,
) -> CompileResult<Option<Value>> {
    let (ty, construction) = match &i.kind {
        InstructionKind::MakeVariant { .. } | InstructionKind::MakeErrorUnion { .. } => (
            value_type(
                f,
                i.result
                    .ok_or_else(|| failure("sum constructor has no result"))?,
            )?,
            true,
        ),
        InstructionKind::ReadVariant { value, .. }
        | InstructionKind::VariantTag { value }
        | InstructionKind::ReadErrorUnion { value, .. }
        | InstructionKind::ErrorTag { value } => (value_type(f, *value)?, false),
        _ => return Ok(None),
    };
    let physical = carrier(ty).map_or(ty, |(storage, _)| storage);
    // Canonical qualified Option keeps its existing instance/None proof and
    // exact old transfer. This path handles the remaining Copy sum geometry.
    if option_address::signature(ty)
        || !needs_state(ty, b, 0)?
        || !ty.is_copy_value()
        || !matches!(physical, Type::Enum { .. } | Type::ErrorUnion { .. })
    {
        return Ok(None);
    }
    let mut out = Value::default();
    match &i.kind {
        InstructionKind::MakeVariant { variant, fields } => {
            let Type::Enum { variants, .. } = physical else {
                return Err(failure("variant constructor has no Enum"));
            };
            let expected = &variants
                .get(*variant as usize)
                .ok_or_else(|| failure("sum constructor tag absent"))?
                .1;
            if fields.len() != expected.len() {
                return Err(failure("sum constructor changes complete field arity"));
            }
            out.enum_variants = enum_address::Variants::constructed(variants.len(), *variant, b)?;
            for (index, (field, ty)) in fields.iter().zip(expected).enumerate() {
                if value_type(f, *field)? != ty {
                    return Err(failure("sum constructor changes complete field Type"));
                }
                replace(
                    &mut out,
                    &[PlaceProjection::Variant(zil_ir::VariantProjection {
                        variant: *variant,
                        index: index as u32,
                    })],
                    &values[field.0 as usize],
                    ty,
                    true,
                    b,
                )?;
            }
        }
        InstructionKind::MakeErrorUnion { value, is_error } => {
            let Type::ErrorUnion { error, result } = physical else {
                return Err(failure("error constructor has no full sum Type"));
            };
            let ty = if *is_error {
                error.as_ref()
            } else {
                result.as_ref()
            };
            if value_type(f, *value)? != ty {
                return Err(failure(
                    "error constructor changes complete active arm Type",
                ));
            }
            out.enum_variants = enum_address::Variants::constructed(2, u32::from(*is_error), b)?;
            replace(
                &mut out,
                &[PlaceProjection::ErrorArm(*is_error)],
                &values[value.0 as usize],
                ty,
                true,
                b,
            )?;
        }
        InstructionKind::ReadVariant {
            value,
            variant,
            index,
        } => {
            let step = PlaceProjection::Variant(zil_ir::VariantProjection {
                variant: *variant,
                index: *index,
            });
            let ty = slot(physical, &[step], b)?;
            if i.result.and_then(|id| f.values.get(id.0 as usize)) != Some(ty) {
                return Err(failure("sum projection changes complete payload Type"));
            }
            out = project(&values[value.0 as usize], &[step], Some(ty), b)?;
        }
        InstructionKind::ReadErrorUnion { value, is_error } => {
            let step = PlaceProjection::ErrorArm(*is_error);
            let ty = slot(physical, &[step], b)?;
            if i.result.and_then(|id| f.values.get(id.0 as usize)) != Some(ty) {
                return Err(failure("error projection changes complete arm Type"));
            }
            out = project(&values[value.0 as usize], &[step], Some(ty), b)?;
        }
        _ => {}
    }
    let _ = construction;
    Ok(Some(out))
}

pub(super) fn needs_state(ty: &Type, b: &mut Budget, depth: usize) -> CompileResult<bool> {
    fee(b, 1)?;
    if depth > b.partial_depth {
        return Err(vec![Diagnostic::limit(
            Phase::Analyze,
            "stored state inventory exceeds finite type depth",
            None,
        )]);
    }
    Ok(match ty {
        Type::Address { .. }
        | Type::Desc(_)
        | Type::Closure(_)
        | Type::Task { .. }
        | Type::Dyn { .. }
        | Type::OwnedDyn { .. } => true,
        Type::BorrowedStorage { storage, .. }
        | Type::Array {
            element: storage, ..
        }
        | Type::Managed(storage)
        | Type::Opaque {
            hidden: storage, ..
        } => needs_state(storage, b, depth + 1)?,
        Type::Tuple(fields) => {
            let mut found = false;
            for field in fields {
                found |= needs_state(field, b, depth + 1)?;
            }
            found
        }
        Type::Record { fields, .. } | Type::Package { fields, .. } => {
            let mut found = false;
            for (_, field) in fields {
                found |= needs_state(field, b, depth + 1)?;
            }
            found
        }
        Type::Enum { variants, .. } => {
            let mut found = false;
            for (_, fields) in variants {
                fee(b, 1)?;
                for field in fields {
                    found |= needs_state(field, b, depth + 1)?;
                }
            }
            found
        }
        Type::ErrorUnion { error, result } => {
            let left = needs_state(error, b, depth + 1)?;
            let right = needs_state(result, b, depth + 1)?;
            left || right
        }
        _ => false,
    })
}
pub(super) fn relevant(ty: &Type, b: &mut Budget, depth: usize) -> CompileResult<bool> {
    needs_state(ty, b, depth)
}

pub(super) fn projection_work(value: &Projection, b: &mut Budget) -> CompileResult<()> {
    permission_clone_work(&value.held, b)?;
    permission_clone_work(&value.store_description, b)?;
    target_work(&value.source_upper, b)?;
    value.option_variants.work(b)?;
    fee(b, usize::from(value.creation_complete.is_some()))
}
fn copy_projection(value: &Projection, b: &mut Budget) -> CompileResult<Projection> {
    projection_work(value, b)?;
    Ok(value.clone())
}
pub(super) fn copy_projections(
    rows: &BTreeMap<Path, Projection>,
    b: &mut Budget,
) -> CompileResult<BTreeMap<Path, Projection>> {
    let mut out = BTreeMap::new();
    for (path, p) in rows {
        fee(b, path.len().saturating_add(1))?;
        out.insert(path.clone(), copy_projection(p, b)?);
    }
    Ok(out)
}
pub(super) fn merge_projection(
    to: &mut Projection,
    from: &Projection,
    b: &mut Budget,
) -> CompileResult<bool> {
    let mut changed = union(&mut to.held, &from.held, b)?;
    changed |= union(&mut to.store_description, &from.store_description, b)?;
    let previous = to.source_upper.len();
    dyn_address::upper_union(&mut to.source_upper, &from.source_upper, b)?;
    changed |= previous != to.source_upper.len();
    changed |= to.option_variants.join(&from.option_variants, b)?;
    if let Some(complete) = from.creation_complete {
        let complete = Some(to.creation_complete.unwrap_or(false) || complete);
        changed |= to.creation_complete != complete;
        to.creation_complete = complete;
    }
    Ok(changed)
}
fn root_projection(value: &Value, b: &mut Budget) -> CompileResult<Projection> {
    target_work(&value.source_upper, b)?;
    value.option_variants.work(b)?;
    Ok(Projection {
        held: copy_permission(&value.held, b)?,
        source_upper: value.source_upper.clone(),
        store_description: copy_permission(&value.store_description, b)?,
        option_variants: value.option_variants.clone(),
        creation_complete: value.creation_complete,
    })
}
pub(super) fn install_projection(
    value: &mut Value,
    path: &[PlaceProjection],
    incoming: &Value,
    strong: bool,
    b: &mut Budget,
) -> CompileResult<()> {
    fee(b, path.len().saturating_add(1))?;
    if path.is_empty() {
        return Err(failure(
            "field auxiliary payload needs a genuine nonempty physical position",
        ));
    }
    replace_pointees(&mut value.pointees, path, &incoming.pointees, strong, b)?;
    replace_heads(
        &mut value.saved_heads,
        path,
        &incoming.saved_heads,
        strong,
        b,
    )?;
    if strong {
        for previous in value.projections.keys() {
            fee(
                b,
                previous.len().saturating_add(path.len()).saturating_add(1),
            )?;
        }
        value
            .projections
            .retain(|previous, _| !previous.starts_with(path));
    }
    let mut add =
        |tail: &[PlaceProjection], payload: Projection, b: &mut Budget| -> CompileResult<()> {
            fee(b, path.len().saturating_add(tail.len()))?;
            let mut full = path.to_vec();
            full.extend(tail);
            ordered(b, value.projections.len(), full.len())?;
            if strong {
                value.projections.insert(full, payload);
            } else {
                merge_projection(value.projections.entry(full).or_default(), &payload, b)?;
            }
            Ok(())
        };
    let root = root_projection(incoming, b)?;
    if !root.held.targets.is_empty()
        || !root.held.loans.is_empty()
        || !root.source_upper.is_empty()
        || !root.store_description.targets.is_empty()
        || !root.store_description.loans.is_empty()
        || root.option_variants.mask() != 0
        || root.creation_complete.is_some()
    {
        add(&[], root, b)?;
    }
    for (tail, p) in &incoming.projections {
        add(tail, copy_projection(p, b)?, b)?;
    }
    union(&mut value.held, &incoming.held, b)?;
    Ok(())
}
fn install_root(value: &mut Value, payload: Projection, b: &mut Budget) -> CompileResult<()> {
    union(&mut value.held, &payload.held, b)?;
    value.source_upper = payload.source_upper;
    value.store_description = payload.store_description;
    value.option_variants = payload.option_variants;
    value.creation_complete = payload.creation_complete;
    Ok(())
}
pub(super) fn take_projections(
    parent: &mut Value,
    index: u32,
    out: &mut Value,
    b: &mut Budget,
) -> CompileResult<()> {
    let prefix = [PlaceProjection::Field(index)];
    out.pointees = project_pointees(&parent.pointees, &prefix, b)?;
    out.saved_heads = project_heads(&parent.saved_heads, &prefix, b)?;
    for path in parent.pointees.keys() {
        fee(b, path.len().saturating_add(1))?;
    }
    parent.pointees.retain(|path, _| !path.starts_with(&prefix));
    for path in &parent.saved_heads {
        fee(b, path.len().saturating_add(1))?;
    }
    parent.saved_heads.retain(|path| !path.starts_with(&prefix));
    let mut selected = Vec::new();
    for (path, p) in &parent.projections {
        fee(b, path.len().saturating_add(1))?;
        if path.starts_with(&prefix) {
            selected.push((path.clone(), copy_projection(p, b)?));
        }
    }
    for (path, p) in selected {
        parent.projections.remove(&path);
        let tail = &path[1..];
        if tail.is_empty() {
            install_root(out, p, b)?;
        } else {
            fee(b, tail.len())?;
            out.projections.insert(tail.to_vec(), p);
        }
    }
    Ok(())
}
pub(super) fn upper(value: &Value, b: &mut Budget) -> CompileResult<Targets> {
    let mut out = Targets::new();
    dyn_address::upper_union(&mut out, &value.source_upper, b)?;
    for payload in value.projections.values() {
        dyn_address::upper_union(&mut out, &payload.source_upper, b)?;
    }
    Ok(out)
}

fn merge_rows<P: Copy + Ord>(
    to: &mut BTreeMap<Vec<P>, Permission>,
    prefix: &[P],
    from: &BTreeMap<Vec<P>, Permission>,
    b: &mut Budget,
) -> CompileResult<()> {
    for (tail, p) in from {
        fee(b, prefix.len().saturating_add(tail.len()))?;
        let mut path = prefix.to_vec();
        path.extend(tail);
        ordered(b, to.len(), path.len())?;
        union(to.entry(path).or_default(), p, b)?;
    }
    Ok(())
}
pub(super) fn recompute_held(value: &mut Value, b: &mut Budget) -> CompileResult<()> {
    let mut held = Permission::default();
    for p in value.environment.values() {
        union(&mut held, p, b)?;
    }
    for p in value.projections.values() {
        union(&mut held, &p.held, b)?;
    }
    value.held = held;
    Ok(())
}
/// Locate the existing non-slot channels below one exact physical position.
/// Slot/environment rows remain in their original, single permission family.
pub(super) fn graft_aux(
    value: &mut Value,
    path: &[PlaceProjection],
    incoming: &Value,
    strong: bool,
    b: &mut Budget,
) -> CompileResult<()> {
    install_projection(value, path, incoming, strong, b)?;

    fee(b, path.len())?;
    let logical: Vec<_> = path
        .iter()
        .copied()
        .map(ContractProjection::Place)
        .collect();
    if strong {
        environment_memory::replace(
            &mut value.task_ports,
            &logical,
            copy_permission_rows(&incoming.task_ports, b)?,
            b,
        )?;
        environment_memory::replace(
            &mut value.dyn_ports,
            &logical,
            copy_permission_rows(&incoming.dyn_ports, b)?,
            b,
        )?;
    } else {
        merge_rows(&mut value.task_ports, &logical, &incoming.task_ports, b)?;
        merge_rows(&mut value.dyn_ports, &logical, &incoming.dyn_ports, b)?;
    }
    Ok(())
}
pub(super) fn source_targets(value: &Value) -> impl Iterator<Item = &Target> {
    std::iter::once(value)
        .chain(value.pointees.values())
        .flat_map(|value| {
            top_permissions(value)
                .flat_map(|p| p.targets.iter())
                .chain(value.source_upper.iter())
                .chain(
                    value
                        .projections
                        .values()
                        .flat_map(|p| p.source_upper.iter()),
                )
        })
}
pub(super) fn substitute_variants(
    value: &mut Value,
    original: &Value,
    arguments: &[ValueId],
    values: &[Value],
    state: &State,
    captures: Option<&Value>,
    b: &mut Budget,
) -> CompileResult<()> {
    for (path, pointee) in &original.pointees {
        ordered(b, value.pointees.len(), path.len())?;
        let mapped = value
            .pointees
            .get_mut(path)
            .ok_or_else(|| failure("summary loses a real saved-head referent"))?;
        substitute_variants(mapped, pointee, arguments, values, state, captures, b)?;
    }
    value.enum_variants = original
        .enum_variants
        .substitute(arguments, values, state, captures, b)?;
    value.option_variants = original.option_variants.substitute(arguments, values, b)?;
    for (path, p) in &original.projections {
        ordered(b, value.projections.len(), path.len())?;
        let actual = value
            .projections
            .get_mut(path)
            .ok_or_else(|| failure("auxiliary summary lost its exact field position"))?;
        actual.option_variants = p.option_variants.substitute(arguments, values, b)?;
    }
    Ok(())
}

/// A pointer stored in an aggregate is the physical leaf row. This does not
/// dereference that pointer or reuse an address SSA's old pointee snapshot.
pub(super) fn pointer_cell(
    state: &State,
    root: &Target,
    b: &mut Budget,
) -> CompileResult<Option<Permission>> {
    if root.retained_port.is_some() || root.heap_element.is_some() || !root.heap_parents.is_empty()
    {
        return Ok(None);
    }
    let Some(value) = root_value(state, root, b)? else {
        return Ok(None);
    };
    if root.stored.is_empty() {
        if root.place.is_empty() {
            return Ok(Some(copy_permission(&value.pointer, b)?));
        }
        ordered(b, value.slots.len(), root.place.len())?;
        return value
            .slots
            .get(&root.place)
            .map(|p| copy_permission(p, b))
            .transpose();
    }
    ordered(b, value.pointees.len(), root.stored.len())?;
    let Some(referent) = value.pointees.get(&root.stored) else {
        return Ok(None);
    };
    if root.place.is_empty() {
        return Ok(Some(copy_permission(&referent.pointer, b)?));
    }
    ordered(b, referent.slots.len(), root.place.len())?;
    referent
        .slots
        .get(&root.place)
        .map(|p| copy_permission(p, b))
        .transpose()
}
pub(super) fn pointer_upper(
    state: &State,
    root: &Target,
    b: &mut Budget,
) -> CompileResult<Option<Targets>> {
    if root.retained_port.is_some() || root.heap_element.is_some() || !root.heap_parents.is_empty()
    {
        return Ok(None);
    }
    let Some(value) = root_value(state, root, b)? else {
        return Ok(None);
    };
    let value = if root.stored.is_empty() {
        value
    } else {
        ordered(b, value.pointees.len(), root.stored.len())?;
        let Some(value) = value.pointees.get(&root.stored) else {
            return Ok(None);
        };
        value
    };
    let path = &root.place;
    let upper = if path.is_empty() {
        &value.source_upper
    } else {
        ordered(b, value.projections.len(), path.len())?;
        let Some(p) = value.projections.get(path) else {
            return Ok(None);
        };
        &p.source_upper
    };
    target_work(upper, b)?;
    Ok(Some(upper.clone()))
}
pub(super) fn install_pointer(
    state: &mut State,
    root: &Target,
    p: &Permission,
    upper: &Targets,
    strong: bool,
    b: &mut Budget,
) -> CompileResult<bool> {
    if root.retained_port.is_some() || root.heap_element.is_some() || !root.heap_parents.is_empty()
    {
        return Ok(false);
    }
    let view = captured_view_local(state, root.base, b)?;
    let external = !root.stored.is_empty()
        || match root.base {
            HoldRoot::Parameter(_) => !root.place.is_empty(),
            HoldRoot::Capture(index) => state
                .captures
                .get(&index)
                .is_none_or(|v| !v.pointer.targets.is_empty()),
            _ => false,
        };
    let value = if external {
        state.external.get_mut(&root.base)
    } else {
        match root.base {
            HoldRoot::Local(local) => state.locals.get_mut(&local),
            HoldRoot::Capture(index) => {
                if let Some(local) = view {
                    state.locals.get_mut(&local)
                } else {
                    state.captures.get_mut(&index)
                }
            }
            _ => None,
        }
    };
    let Some(value) = value else {
        return Ok(false);
    };
    let value = if root.stored.is_empty() {
        value
    } else {
        ordered(b, value.pointees.len(), root.stored.len())?;
        let Some(value) = value.pointees.get_mut(&root.stored) else {
            return Ok(false);
        };
        value
    };
    fee(b, root.place.len())?;
    let path = &root.place;
    if path.is_empty() {
        if strong {
            value.pointer = copy_permission(p, b)?;
            target_work(upper, b)?;
            value.source_upper = upper.clone();
        } else {
            union(&mut value.pointer, p, b)?;
            dyn_address::upper_union(&mut value.source_upper, upper, b)?;
        }
    } else {
        let rows = &mut value.slots;
        ordered(b, rows.len(), path.len())?;
        if strong {
            fee(b, path.len())?;
            rows.insert(path.clone(), copy_permission(p, b)?);
        } else {
            fee(b, path.len())?;
            union(rows.entry(path.clone()).or_default(), p, b)?;
        }
        let payload = Value {
            source_upper: {
                target_work(upper, b)?;
                upper.clone()
            },
            ..Value::default()
        };
        install_projection(value, &path, &payload, strong, b)?;
        recompute_held(value, b)?;
    }
    b.stored_slot_updates = true;
    Ok(true)
}

// Flat within one finite inline frame. Values are current aggregate cells;
// following a Target never allocates or nests another memory snapshot.
fn finite_pointee(value: &Value) -> CompileResult<()> {
    if !value.pointees.is_empty() {
        return Err(failure(
            "referent output cannot recursively own a saved-head memory graph",
        ));
    }
    Ok(())
}
pub(super) fn copy_pointees(
    rows: &BTreeMap<Path, Value>,
    b: &mut Budget,
) -> CompileResult<BTreeMap<Path, Value>> {
    let mut out = BTreeMap::new();
    for (path, value) in rows {
        finite_pointee(value)?;
        fee(b, path.len().saturating_add(1))?;
        out.insert(path.clone(), snapshot(value, b)?);
    }
    Ok(out)
}
fn project_pointees(
    rows: &BTreeMap<Path, Value>,
    path: &[PlaceProjection],
    b: &mut Budget,
) -> CompileResult<BTreeMap<Path, Value>> {
    let mut out = BTreeMap::new();
    for (full, value) in rows {
        finite_pointee(value)?;
        fee(b, full.len().saturating_add(path.len()).saturating_add(1))?;
        if let Some(tail) = full.strip_prefix(path) {
            fee(b, tail.len())?;
            out.insert(tail.to_vec(), snapshot(value, b)?);
        }
    }
    Ok(out)
}
fn replace_pointees(
    rows: &mut BTreeMap<Path, Value>,
    path: &[PlaceProjection],
    incoming: &BTreeMap<Path, Value>,
    strong: bool,
    b: &mut Budget,
) -> CompileResult<()> {
    for full in rows.keys() {
        fee(b, full.len().saturating_add(path.len()).saturating_add(1))?;
    }
    if strong {
        rows.retain(|full, _| !full.starts_with(path));
    }
    for (tail, value) in incoming {
        finite_pointee(value)?;
        fee(b, path.len().saturating_add(tail.len()).saturating_add(1))?;
        let mut full = path.to_vec();
        full.extend(tail);
        if strong {
            rows.insert(full, snapshot(value, b)?);
        } else {
            merge(rows.entry(full).or_default(), value, b)?;
        }
    }
    Ok(())
}
pub(super) fn transport<F>(value: &Value, b: &mut Budget, mut source: F) -> CompileResult<Value>
where
    F: FnMut(&Target, &mut Budget) -> CompileResult<Permission>,
{
    let mut out = transport_top(value, b, &mut source)?;
    for (path, value) in &value.pointees {
        finite_pointee(value)?;
        fee(b, path.len().saturating_add(1))?;
        out.pointees
            .insert(path.clone(), transport_top(value, b, &mut source)?);
    }
    Ok(out)
}
pub(super) fn graft_capture_aux(
    value: &mut Value,
    index: u32,
    incoming: &Value,
    strong: bool,
    b: &mut Budget,
) -> CompileResult<()> {
    let physical = [PlaceProjection::Field(index)];
    let logical = [ContractProjection::Capture(index)];
    install_projection(value, &physical, incoming, strong, b)?;

    if strong {
        environment_memory::replace(
            &mut value.task_ports,
            &logical,
            copy_permission_rows(&incoming.task_ports, b)?,
            b,
        )?;
        environment_memory::replace(
            &mut value.dyn_ports,
            &logical,
            copy_permission_rows(&incoming.dyn_ports, b)?,
            b,
        )?;
    } else {
        merge_rows(&mut value.task_ports, &logical, &incoming.task_ports, b)?;
        merge_rows(&mut value.dyn_ports, &logical, &incoming.dyn_ports, b)?;
    }
    Ok(())
}
pub(super) fn seed_shape(
    module: &TypedModule,
    ty: &Type,
    value: &Value,
    prior: Option<&Value>,
    b: &mut Budget,
) -> CompileResult<Option<Value>> {
    let roots = environment_roots(ty, b)?;
    if roots.is_empty() {
        return Ok(None);
    }
    let mut out = if matches!(ty, Type::Address { .. }) {
        prior
            .map(|p| snapshot(p, b))
            .transpose()?
            .unwrap_or_default()
    } else {
        snapshot(value, b)?
    };
    out.pointer = Permission::default();
    out.environment = copy_permission_rows(&value.environment, b)?;
    out.saved_heads = value_heads(module, ty, b)?;
    for (_, prefix) in &roots {
        let tags = value.enum_variants.project(prefix, b)?;
        out.enum_variants.replace(prefix, &tags, true, b)?;
    }
    let mut pending = roots
        .into_iter()
        .map(|(id, path)| (&module.closures[id.0 as usize], path, 0usize))
        .collect::<Vec<_>>();
    while let Some((d, prefix, depth)) = pending.pop() {
        if depth > b.partial_depth {
            return Err(vec![Diagnostic::limit(
                Phase::Analyze,
                "saved-head inventory exceeds finite typed depth",
                None,
            )]);
        }
        for (index, ty) in d.captures.iter().enumerate() {
            fee(b, prefix.len().saturating_add(1))?;
            let mut head = prefix.clone();
            head.push(PlaceProjection::Field(index as u32));
            if let Type::Address { inner, .. } = ty
                && context(inner)
            {
                let declared = rows(inner, b)?;
                let mut referent = Value {
                    enum_variants: value.enum_variants.project(&head, b)?,
                    environment: environment_memory::project(&value.environment, &head, b)?,
                    saved_heads: value_heads(module, inner, b)?,
                    ..Value::default()
                };
                referent.environment.remove(&Vec::new());
                for (path, _) in declared {
                    ordered(b, referent.environment.len(), path.len())?;
                    if let Some(p) = referent.environment.remove(&path) {
                        referent.slots.insert(path, p);
                    }
                }
                strip_pointee_snapshots(&mut referent, b)?;
                ordered(b, out.pointees.len(), head.len())?;
                out.pointees.insert(head.clone(), referent);
                for path in out.environment.keys() {
                    fee(b, path.len().saturating_add(head.len()).saturating_add(1))?;
                }
                out.environment
                    .retain(|path, _| !path.starts_with(&head) || path == &head);
                out.enum_variants
                    .replace(&head, &enum_address::Variants::default(), true, b)?;
                // Only the already declared inline layout beyond this
                // boundary contributes heads to the same owner directory.
                for (child, relative) in environment_roots(inner, b)? {
                    fee(b, head.len().saturating_add(relative.len()))?;
                    let mut nested = head.clone();
                    nested.extend(relative);
                    pending.push((&module.closures[child.0 as usize], nested, depth + 1));
                }
            } else if !matches!(ty, Type::Address { .. }) {
                for (child, relative) in environment_roots(ty, b)? {
                    fee(b, head.len().saturating_add(relative.len()))?;
                    let mut nested = head.clone();
                    nested.extend(relative);
                    pending.push((&module.closures[child.0 as usize], nested, depth + 1));
                }
            }
        }
    }
    strip_pointee_snapshots(&mut out, b)?;
    Ok(Some(out))
}

pub(super) fn copy_heads(heads: &BTreeSet<Path>, b: &mut Budget) -> CompileResult<BTreeSet<Path>> {
    for head in heads {
        fee(b, head.len().saturating_add(1))?;
    }
    Ok(heads.clone())
}
fn project_heads(
    heads: &BTreeSet<Path>,
    path: &[PlaceProjection],
    b: &mut Budget,
) -> CompileResult<BTreeSet<Path>> {
    let mut out = BTreeSet::new();
    for head in heads {
        fee(b, head.len().saturating_add(path.len()).saturating_add(1))?;
        if let Some(tail) = head.strip_prefix(path) {
            fee(b, tail.len())?;
            out.insert(tail.to_vec());
        }
    }
    Ok(out)
}
fn replace_heads(
    heads: &mut BTreeSet<Path>,
    path: &[PlaceProjection],
    incoming: &BTreeSet<Path>,
    strong: bool,
    b: &mut Budget,
) -> CompileResult<()> {
    for head in heads.iter() {
        fee(b, head.len().saturating_add(path.len()).saturating_add(1))?;
    }
    if strong {
        heads.retain(|head| !head.starts_with(path));
    }
    for tail in incoming {
        fee(b, path.len().saturating_add(tail.len()))?;
        let mut head = path.to_vec();
        head.extend(tail);
        heads.insert(head);
    }
    Ok(())
}
pub(super) fn frame_heads(
    module: &TypedModule,
    d: &ClosureDefinition,
    b: &mut Budget,
) -> CompileResult<BTreeSet<Path>> {
    let mut out = BTreeSet::new();
    let mut pending = vec![(d, Vec::new(), 0usize)];
    while let Some((d, prefix, depth)) = pending.pop() {
        if depth > b.partial_depth {
            return Err(vec![Diagnostic::limit(
                Phase::Analyze,
                "saved-head query inventory exceeds finite depth",
                None,
            )]);
        }
        for (index, ty) in d.captures.iter().enumerate() {
            fee(b, prefix.len().saturating_add(1))?;
            let mut head = prefix.clone();
            head.push(PlaceProjection::Field(index as u32));
            if let Type::Address { inner, .. } = ty
                && context(inner)
            {
                out.insert(head);
            } else if !matches!(ty, Type::Address { .. }) {
                for (child, relative) in environment_roots(ty, b)? {
                    fee(b, head.len().saturating_add(relative.len()))?;
                    let mut path = head.clone();
                    path.extend(relative);
                    pending.push((&module.closures[child.0 as usize], path, depth + 1));
                }
            }
        }
    }
    Ok(out)
}

pub(super) fn check_variants(
    value: &Value,
    arguments: &[ValueId],
    values: &[Value],
    state: &State,
    captures: Option<&Value>,
    b: &mut Budget,
) -> CompileResult<()> {
    value
        .enum_variants
        .check_inputs(arguments, values, state, captures, b)?;
    for (head, value) in &value.pointees {
        fee(b, head.len().saturating_add(1))?;
        value
            .enum_variants
            .check_inputs(arguments, values, state, captures, b)?;
    }
    Ok(())
}

pub(super) fn value_heads(
    module: &TypedModule,
    ty: &Type,
    b: &mut Budget,
) -> CompileResult<BTreeSet<Path>> {
    let ty = if let Type::Address { inner, .. } = ty {
        inner.as_ref()
    } else {
        ty
    };
    let mut out = BTreeSet::new();
    for (id, prefix) in environment_roots(ty, b)? {
        for head in frame_heads(module, &module.closures[id.0 as usize], b)? {
            fee(b, prefix.len().saturating_add(head.len()))?;
            let mut path = prefix.clone();
            path.extend(head);
            out.insert(path);
        }
    }
    Ok(out)
}
pub(super) fn strip_pointee_snapshots(value: &mut Value, b: &mut Budget) -> CompileResult<()> {
    for head in &value.saved_heads {
        for path in value.environment.keys() {
            fee(b, path.len().saturating_add(head.len()).saturating_add(1))?;
        }
        value
            .environment
            .retain(|path, _| !path.starts_with(head) || path == head);
        value
            .enum_variants
            .replace(head, &enum_address::Variants::default(), true, b)?;
    }
    recompute_held(value, b)
}

/// A strong update needs one actual destination across the whole staged
/// operation. Different saved heads can denote the same real caller cell.
pub(super) fn commit_updates(
    state: &mut State,
    updates: Vec<(Permission, Value, &Type)>,
    b: &mut Budget,
) -> CompileResult<()> {
    fee(b, updates.len())?;
    let mut strong = Vec::with_capacity(updates.len());
    for (index, (address, _, _)) in updates.iter().enumerate() {
        let mut unique = address.targets.len() == 1;
        for (other, (right, _, _)) in updates.iter().enumerate() {
            if index == other {
                continue;
            }
            for a in &address.targets {
                for z in &right.targets {
                    fee(
                        b,
                        target_width(a)
                            .saturating_add(target_width(z))
                            .saturating_add(1),
                    )?;
                    if a.base == z.base
                        && a.stored == z.stored
                        && a.retained_port == z.retained_port
                        && a.heap_element == z.heap_element
                        && a.heap_parents == z.heap_parents
                        && (a.place.starts_with(&z.place) || z.place.starts_with(&a.place))
                    {
                        unique = false;
                    }
                }
            }
        }
        strong.push(unique);
    }
    for ((address, value, ty), strong) in updates.into_iter().zip(strong) {
        if !address.targets.is_empty() {
            write(state, &address, &value, ty, strong, b)?;
        }
    }
    Ok(())
}

/// Flatten effects from an existing root owner into its one head directory.
/// This moves recorded cells; it never follows or expands a Target edge.
pub(super) fn add_pointee_effect(
    to: &mut Value,
    head: Path,
    mut cell: Value,
    b: &mut Budget,
) -> CompileResult<()> {
    let children = std::mem::take(&mut cell.pointees);
    ordered(b, to.pointees.len(), head.len())?;
    fee(b, head.len().saturating_add(1))?;
    merge(to.pointees.entry(head.clone()).or_default(), &cell, b)?;
    for (tail, child) in children {
        fee(b, head.len().saturating_add(tail.len()).saturating_add(1))?;
        let mut full = head.clone();
        full.extend(tail);
        finite_pointee(&child)?;
        ordered(b, to.pointees.len(), full.len())?;
        merge(to.pointees.entry(full).or_default(), &child, b)?;
    }
    Ok(())
}
