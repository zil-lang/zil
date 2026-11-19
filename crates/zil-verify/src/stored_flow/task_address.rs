//! Independent external Task-result permission reconstruction.
use super::*;
mod carriers;
mod inputs;
mod results;

pub(super) fn port_relation(
    module: &TypedModule,
    f: &TypedFunction,
    base: StorageBase,
    path: &[ContractProjection],
    b: &mut Budget,
) -> Check<StorageBinder> {
    carriers::relation(module, f, base, path, b)
}
/// Complete existing Task carrier paths, shared by real Return snapshots.
/// A path is an obligation to find a permission, never an initialized port.
pub(super) fn input_ports(
    module: &TypedModule,
    ty: &Type,
    b: &mut Budget,
) -> Check<Vec<(Vec<ContractProjection>, StoredBorrowMode)>> {
    let mut out = Vec::new();
    for (mut path, port, mode) in carriers::paths(module, ty, b)? {
        b.spend(2)?;
        path.push(ContractProjection::RetainedPort(port));
        out.push((path, mode));
    }
    Ok(out)
}
pub(super) fn has_carrier(module: &TypedModule, ty: &Type, b: &mut Budget) -> Check<bool> {
    Ok(!carriers::paths(module, ty, b)?.is_empty())
}
fn task_type(ty: &Type) -> Option<(&Type, &zil_ir::RetainedLifeShape)> {
    match ty {
        Type::Task {
            result,
            retained_life,
        } => Some((result, retained_life)),
        Type::Managed(inner) | Type::Desc(inner) | Type::Address { inner, .. } => task_type(inner),
        Type::ErrorUnion { result, .. } => task_type(result),
        _ => None,
    }
}
pub(super) fn shape(ty: &Type) -> bool {
    task_type(ty).is_some()
}
pub(super) fn admitted_value(ty: &Type) -> bool {
    shape(ty)
}
pub(super) fn owner_instruction(f: &TypedFunction, kind: &InstructionKind) -> bool {
    match kind {
        InstructionKind::TakeLocal { local }
        | InstructionKind::DropManaged { local }
        | InstructionKind::DescribeLocal { local } => {
            f.locals.get(local.0 as usize).is_some_and(|l| shape(&l.ty))
        }
        InstructionKind::ReadDesc { desc, .. } => f.values.get(desc.0 as usize).is_some_and(shape),
        _ => false,
    }
}
pub(super) fn capture_instruction(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    b: &mut Budget,
) -> Check<bool> {
    let (index, describe, borrowing) = match i.kind {
        InstructionKind::TakeCapture { index } => (index, false, false),
        InstructionKind::DescribeCapture { index } => (index, true, false),
        InstructionKind::BorrowCapture {
            index,
            mutable: true,
        } => (index, false, true),
        _ => return Ok(false),
    };
    let Some(d) = f
        .environment
        .and_then(|id| module.closures.get(id.0 as usize))
    else {
        return Ok(false);
    };
    let Some(actual) = d.captures.get(index as usize) else {
        return Err(err("actual Task capture absent"));
    };
    if !matches!(actual,Type::Managed(inner) if matches!(inner.as_ref(),Type::Task{..})) {
        return Ok(false);
    }
    type_work(actual, &b.limits.clone(), b)?;
    let out = i
        .result
        .and_then(|value| f.values.get(value.0 as usize))
        .ok_or_else(|| err("consuming Task capture result absent"))?;
    if borrowing {
        if d.kind != CallableKind::FnMut
            || !matches!(out, Type::Address { mutable: true, inner } if inner.as_ref() == actual)
        {
            return Err(err(
                "exclusive Task capture view differs from its actual mutable entry",
            ));
        }
        let Type::Managed(payload) = actual else {
            unreachable!()
        };
        super::super::tasks::require_closed_context(payload, Some(i.span))?;
        return Ok(true);
    }
    if describe {
        let Type::Managed(payload) = actual else {
            unreachable!()
        };
        type_work(out, &b.limits.clone(), b)?;
        if !matches!(out,Type::Desc(inner) if inner.as_ref()==payload.as_ref()) {
            return Err(err(
                "Task description does not refer to its actual captured payload",
            ));
        }
        return Ok(true);
    }
    if d.kind != CallableKind::FnOnce || out != actual {
        return Err(err("Task capture must consume exact actual FnOnce field"));
    }
    if let Type::Managed(payload) = actual {
        super::super::tasks::require_closed_context(payload, Some(i.span))?;
    }
    Ok(true)
}

pub(super) fn borrow_capture(index: u32, state: &Frame, b: &mut Budget) -> Check<Payload> {
    let original = state
        .captures
        .get(index as usize)
        .ok_or_else(|| err("actual captured Task payload is absent"))?;
    let mut output = copy_payload(original, b)?;
    b.spend(2)?;
    output.address = Permission {
        sources: vec![Source {
            static_dependency: StaticProvenance::NotStatic,
            retained_port: None,
            base: HoldRoot::Capture(index),
            leaf: vec![],
            inline: vec![],
            heap_parents: vec![],
            heap_element: None,
        }],
        loans: state.receiver.into_iter().collect(),
    };
    Ok(output)
}
pub(super) fn operation(kind: &InstructionKind) -> bool {
    matches!(
        kind,
        InstructionKind::PlaceTagLocal { .. }
            | InstructionKind::TakeVariantField { .. }
            | InstructionKind::TakeErrorUnion { .. }
            | InstructionKind::DropErrorUnion { .. }
            | InstructionKind::DropEnum { .. }
            | InstructionKind::TaskCreate { .. }
            | InstructionKind::TaskCreateOwned { .. }
            | InstructionKind::TaskPoll { .. }
            | InstructionKind::TaskYield
            | InstructionKind::TaskValid { .. }
            | InstructionKind::TaskRequestCancel { .. }
            | InstructionKind::TaskPollClose { .. }
            | InstructionKind::TaskFinishDestroy { .. }
            | InstructionKind::MakeErrorUnion { .. }
            | InstructionKind::ReadErrorUnion { .. }
            | InstructionKind::ErrorTag { .. }
    )
}
fn port_path(places: &[PlaceProjection], port: u32) -> Vec<ContractProjection> {
    let mut out = places
        .iter()
        .copied()
        .map(ContractProjection::Place)
        .collect::<Vec<_>>();
    out.push(ContractProjection::RetainedPort(port));
    out
}
pub(super) fn work(p: &Payload, b: &mut Budget) -> Check<()> {
    if !b.stored_task_flow() {
        return Ok(());
    }
    b.spend(p.task_ports.len())?;
    for (path, permission) in &p.task_ports {
        b.spend(path.len())?;
        permission_work(permission, b)?;
    }
    Ok(())
}
pub(super) fn insert(
    out: &mut Payload,
    path: &[ContractProjection],
    permission: &Permission,
    b: &mut Budget,
) -> Check<bool> {
    b.spend(amount(
        out.task_ports.len().saturating_add(1),
        path.len().saturating_add(1),
    )?)?;
    if let Some((_, old)) = out.task_ports.iter_mut().find(|(p, _)| p == path) {
        join_permission(old, permission, b)
    } else {
        permission_work(permission, b)?;
        out.task_ports.push((path.to_vec(), permission.clone()));
        Ok(true)
    }
}
pub(super) fn join(out: &mut Payload, from: &Payload, b: &mut Budget) -> Check<bool> {
    if !b.stored_task_flow() {
        return Ok(false);
    }
    let mut changed = false;
    for (path, p) in &from.task_ports {
        changed |= insert(out, path, p, b)?;
    }
    Ok(changed)
}
pub(super) fn seed(
    module: &TypedModule,
    ty: &Type,
    base: HoldRoot,
    out: &mut Payload,
    loans: &mut Vec<Loan>,
    b: &mut Budget,
) -> Check<()> {
    if !b.stored_task_flow() {
        return Ok(());
    }
    for (carrier, port, mode) in carriers::paths(module, ty, b)? {
        b.spend(carrier.len().saturating_mul(3).saturating_add(3))?;
        let source = Source {
            static_dependency: StaticProvenance::NotStatic,
            base,
            leaf: Vec::new(),
            inline: Vec::new(),
            heap_parents: Vec::new(),
            heap_element: None,
            retained_port: Some(PortOrigin::Task {
                carrier: carrier.clone(),
                port,
            }),
        };
        let id = loans.len();
        loans.push(Loan {
            receiver_head: false,
            sources: vec![source.clone()],
            parents: vec![],
            exclusive: mode == StoredBorrowMode::ExclusiveAddress,
        });
        b.stored_exclusive_loans |= mode == StoredBorrowMode::ExclusiveAddress;
        let mut path = carrier;
        path.push(ContractProjection::RetainedPort(port));
        insert(
            out,
            &path,
            &Permission {
                sources: vec![source],
                loans: vec![id],
            },
            b,
        )?;
    }
    Ok(())
}
pub(super) fn create_environment(
    captures: &[ValueId],
    values: &[Payload],
    out: &mut Payload,
    b: &mut Budget,
) -> Check<()> {
    if !b.stored_task_flow() {
        return Ok(());
    }
    b.spend(captures.len())?;
    for (index, value) in captures.iter().enumerate() {
        let value = values
            .get(value.0 as usize)
            .ok_or_else(|| err("Task environment actual capture absent"))?;
        for (path, permission) in &value.task_ports {
            b.spend(path.len().saturating_add(1))?;
            let mut selected = vec![ContractProjection::Capture(index as u32)];
            selected.extend_from_slice(path);
            insert(out, &selected, permission, b)?;
        }
    }
    Ok(())
}
pub(super) fn compatible_capture_port(
    module: &TypedModule,
    id: zil_ir::ClosureId,
    position: &zil_ir::StoragePosition,
    source: &Type,
    b: &mut Budget,
) -> Check<bool> {
    let Some((ContractProjection::RetainedPort(port), carrier)) = position.path.split_last() else {
        return Ok(false);
    };
    let StorageBase::Capture(capture) = position.base else {
        return Err(err("Task capture endpoint has no actual capture"));
    };
    b.spend(carrier.len().saturating_add(1))?;
    let mut selected = vec![ContractProjection::Capture(capture)];
    selected.extend_from_slice(carrier);
    let root = Type::Closure(id);
    let actual = carriers::endpoint(module, &root, &selected, b)?;
    let Type::Task { retained_life, .. } = actual else {
        return Err(err("returned opaque Task leaf has non-Task endpoint"));
    };
    let leaf = &retained_life
        .ports
        .get(*port as usize)
        .ok_or_else(|| err("returned Task port absent"))?
        .leaf;
    type_work(leaf, &b.limits.clone(), b)?;
    type_work(source, &b.limits.clone(), b)?;
    let compatible = leaf == source
        || matches!((source,leaf),(Type::Address{mutable:from,inner:a},Type::Address{mutable:to,inner:z})if (!*to||*from)&&a==z);
    if !compatible {
        return Err(err(
            "returned Task data port differs from actual source leaf",
        ));
    }
    Ok(true)
}
pub(super) fn environment_task_leaf(
    module: &TypedModule,
    id: zil_ir::ClosureId,
    position: &zil_ir::StoragePosition,
    ty: &Type,
    b: &mut Budget,
) -> Check<bool> {
    let Some((ContractProjection::RetainedPort(port), carrier)) = position.path.split_last() else {
        return Ok(false);
    };
    let StorageBase::Capture(index) = position.base else {
        return Err(err("Task environment port lacks real capture"));
    };
    b.spend(carrier.len().saturating_add(1))?;
    let mut complete = vec![ContractProjection::Capture(index)];
    complete.extend_from_slice(carrier);
    let root = Type::Closure(id);
    let endpoint = carriers::endpoint(module, &root, &complete, b)?;
    let Type::Task { retained_life, .. } = endpoint else {
        return Ok(false);
    };
    if !b.stored_task_flow() {
        return Err(err(
            "opaque Task environment leaf has no actual Task consumer",
        ));
    }
    crate::tasks::require_closed_context(endpoint, None)?;
    let definition = crate::closures::definition(module, id, None)?;
    let entry = module
        .functions
        .get(definition.function.0 as usize)
        .filter(|entry| entry.id == definition.function && entry.environment == Some(id))
        .ok_or_else(|| err("Task environment leaf has no exact actual closure entry"))?;
    if entry.storage_life.is_none() || entry.storage_life != definition.storage_life {
        return Err(err(
            "Task environment leaf changes its complete actual closure scope",
        ));
    }
    if !matches!(
        carriers::relation(
            module,
            entry,
            StorageBase::Capture(index),
            &position.path,
            b
        )?,
        StorageBinder::Fixed(_)
    ) {
        return Err(err(
            "Task environment port cannot retain an invocation-local class",
        ));
    }
    let actual = &retained_life
        .ports
        .get(*port as usize)
        .ok_or_else(|| err("Task environment port absent"))?
        .leaf;
    type_work(actual, &b.limits.clone(), b)?;
    type_work(ty, &b.limits.clone(), b)?;
    if actual != ty {
        return Err(err("Task environment data leaf differs from actual port"));
    }
    Ok(true)
}
pub(super) fn select_carrier(
    value: &Payload,
    physical: &[PlaceProjection],
    carrier: &[ContractProjection],
    port: u32,
    b: &mut Budget,
) -> Check<Permission> {
    b.spend(
        physical
            .len()
            .saturating_add(carrier.len())
            .saturating_add(1),
    )?;
    let mut wanted = port_path(physical, port);
    wanted.splice(wanted.len() - 1..wanted.len() - 1, carrier.iter().cloned());
    b.spend(amount(
        value.task_ports.len().saturating_add(1),
        wanted.len().saturating_add(1),
    )?)?;
    match value.task_ports.iter().find(|(path, _)| path == &wanted) {
        Some((_, p)) => {
            permission_work(p, b)?;
            Ok(p.clone())
        }
        None => Ok(Permission::default()),
    }
}
#[allow(clippy::too_many_arguments)]
pub(super) fn select_actual(
    module: &TypedModule,
    f: &TypedFunction,
    value: ValueId,
    payload: &Payload,
    state: &Frame,
    carrier: &[ContractProjection],
    port: u32,
    b: &mut Budget,
) -> Check<Permission> {
    let endpoint = carriers::endpoint(module, vt(f, value)?, carrier, b)?;
    let Type::Task { retained_life, .. } = endpoint else {
        return Err(err("Task source selector has another actual endpoint kind"));
    };
    if retained_life.ports.get(port as usize).is_none() {
        return Err(err(
            "Task source selector exceeds its complete actual port directory",
        ));
    }
    if !matches!(vt(f, value)?, Type::Address { .. }) {
        return select_carrier(payload, &[], carrier, port, b);
    }
    // A borrowed owner observes its current actual holder. A moved value keeps
    // its own SSA payload; neither route borrows another same-shaped carrier.
    let mut out = Permission::default();
    for source in &payload.address.sources {
        if source.retained_port.is_some()
            || !source.leaf.is_empty()
            || source.heap_element.is_some()
            || !source.heap_parents.is_empty()
        {
            return Err(err(
                "Task owner address has no complete actual holder route",
            ));
        }
        let holder = memory_holder(state, source.base, b)?
            .ok_or_else(|| err("Task owner address selects an absent current holder"))?;
        b.spend(source.inline.len().saturating_add(carrier.len()))?;
        let mut path = source
            .inline
            .iter()
            .copied()
            .map(ContractProjection::Place)
            .collect::<Vec<_>>();
        path.extend_from_slice(carrier);
        join_permission(&mut out, &select_carrier(holder, &[], &path, port, b)?, b)?;
    }
    // Receiver/head loans are checked separately. They do not become data
    // targets or permanently pin the Task to a temporary invocation borrow.
    Ok(out)
}
#[allow(clippy::too_many_arguments)]
pub(super) fn substitute_capture(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    source: &Source,
    definition: &ClosureDefinition,
    values: &[Payload],
    state: &Frame,
    b: &mut Budget,
) -> Check<Option<Permission>> {
    let Some(PortOrigin::Task { carrier, port }) = &source.retained_port else {
        return Ok(None);
    };
    let HoldRoot::Capture(index) = source.base else {
        return Err(err("captured Task source has no actual capture root"));
    };
    if source.heap_element.is_some() || !source.heap_parents.is_empty() {
        return Err(err(
            "captured Task source mixes opaque and heap target namespaces",
        ));
    }
    let (actual, _) = stored_callables::called(module, f, &i.kind)
        .ok_or_else(|| err("captured Task source has no actual callable invocation"))?;
    if actual.id != definition.id || actual.function != definition.function {
        return Err(err(
            "captured Task source selects another actual closure entry",
        ));
    }
    let ty = definition
        .captures
        .get(index as usize)
        .ok_or_else(|| err("captured Task source selects an absent actual capture"))?;
    b.spend(
        source
            .leaf
            .len()
            .saturating_add(carrier.len())
            .saturating_add(1),
    )?;
    let mut local = source
        .leaf
        .iter()
        .copied()
        .map(ContractProjection::Place)
        .collect::<Vec<_>>();
    local.extend_from_slice(carrier);
    let endpoint = carriers::endpoint(module, ty, &local, b)?;
    let Type::Task { retained_life, .. } = endpoint else {
        return Err(err(
            "captured Task port has a different actual endpoint kind",
        ));
    };
    crate::tasks::require_closed_context(endpoint, None)?;
    if retained_life.ports.get(*port as usize).is_none() {
        return Err(err(
            "captured Task port exceeds its complete actual directory",
        ));
    }
    let mut path = vec![ContractProjection::Capture(index)];
    path.extend(local);
    let mut permission = match &i.kind {
        InstructionKind::CallClosure { callee, .. }
        | InstructionKind::CallClosureOnce { callee, .. } => {
            let payload = values
                .get(callee.0 as usize)
                .ok_or_else(|| err("consumed Task environment has no actual SSA payload"))?;
            select_carrier(payload, &[], &path, *port, b)?
        }
        InstructionKind::CallClosureShared { callee, .. }
        | InstructionKind::CallClosureMut { callee, .. } => {
            let payload = state
                .locals
                .get(callee.0 as usize)
                .ok_or_else(|| err("Task environment has no current actual local"))?;
            select_carrier(payload, &[], &path, *port, b)?
        }
        InstructionKind::CallClosureBorrowed { callee, .. } => {
            let payload = values
                .get(callee.0 as usize)
                .ok_or_else(|| err("borrowed Task environment has no actual SSA payload"))?;
            select_actual(module, f, *callee, payload, state, &path, *port, b)?
        }
        _ => {
            return Err(err(
                "captured Task port has no actual closure call consumer",
            ));
        }
    };
    for actual in &mut permission.sources {
        b.spend(source.inline.len())?;
        actual.inline.extend_from_slice(&source.inline);
        if source.static_dependency == StaticProvenance::UpperOnly {
            actual.static_dependency = StaticProvenance::UpperOnly;
        }
    }
    Ok(Some(permission))
}
pub(super) fn select(
    p: &Payload,
    leaf: &[PlaceProjection],
    port: u32,
    b: &mut Budget,
) -> Check<Permission> {
    let path = port_path(leaf, port);
    b.spend(amount(
        p.task_ports.len().saturating_add(1),
        path.len().saturating_add(1),
    )?)?;
    if let Some((_, permission)) = p.task_ports.iter().find(|(p, _)| p == &path) {
        permission_work(permission, b)?;
        Ok(permission.clone())
    } else {
        Ok(Permission::default())
    }
}
#[allow(clippy::too_many_arguments)]
pub(super) fn call(
    module: &TypedModule,
    f: &TypedFunction,
    summary: &Summary,
    args: &[ValueId],
    values: &[Payload],
    state: &Frame,
    out: &mut Payload,
    b: &mut Budget,
) -> Check<()> {
    if !b.stored_task_flow() {
        return Ok(());
    }
    for (path, sources) in &summary.task_ports {
        let mut p = Permission::default();
        for s in sources {
            join_permission(
                &mut p,
                &instantiate(module, f, s, args, values, state, b)?,
                b,
            )?;
        }
        insert(out, path, &p, b)?;
    }
    Ok(())
}
pub(super) fn source_relation(
    module: &TypedModule,
    f: &TypedFunction,
    source: &Source,
    b: &mut Budget,
) -> Check<StorageBinder> {
    let base = match source.base {
        HoldRoot::Parameter(index) => StorageBase::Parameter(index),
        HoldRoot::Capture(index) => {
            let id = f
                .environment
                .ok_or_else(|| err("Task result capture has no actual environment"))?;
            let d = module
                .closures
                .get(id.0 as usize)
                .ok_or_else(|| err("Task result capture definition is absent"))?;
            if d.function != f.id
                || d.kind != CallableKind::FnOnce
                || !d.parameters.is_empty()
                || d.captures.get(index as usize).is_none()
                || !matches!(&d.result,Type::Managed(task) if matches!(task.as_ref(),Type::Task{..}))
            {
                return Err(err(
                    "Task result capture differs from its actual consuming entry",
                ));
            }
            StorageBase::Capture(index)
        }
        _ => return Err(err("Task result source is activation-local")),
    };
    match &source.retained_port {
        Some(PortOrigin::Task { carrier, port }) => {
            let mut path = port_path(&source.leaf, *port);
            b.spend(path.len().saturating_add(carrier.len()))?;
            path.splice(path.len() - 1..path.len() - 1, carrier.iter().cloned());
            carriers::relation(module, f, base, &path, b)
        }
        Some(PortOrigin::Dyn { carrier, port }) => {
            b.spend(source.leaf.len().saturating_add(carrier.len()))?;
            let path = source
                .leaf
                .iter()
                .copied()
                .map(ContractProjection::Place)
                .chain(carrier.iter().copied())
                .collect::<Vec<_>>();
            dyn_returns::source_relation_at(module, f, base, &path, *port, b)
        }
        Some(PortOrigin::Reference { .. }) => reference_cells::relation(module, f, source, b),
        None => relation(f, base, &source.leaf, b),
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn invoke(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    values: &[Payload],
    summaries: &[Summary],
    state: &Frame,
    b: &mut Budget,
) -> Check<Payload> {
    let InstructionKind::CallClosureOnce { callee, arguments } = &i.kind else {
        return Err(err("owning Task result needs an actual consuming call"));
    };
    let Type::Closure(id) = vt(f, *callee)? else {
        return Err(err("Task-returning call has no actual closure receiver"));
    };
    let d = module
        .closures
        .get(id.0 as usize)
        .filter(|d| super::super::stored_callables::task_result(d))
        .ok_or_else(|| err("Task-returning call has no actual consuming definition"))?;
    let mut out = Payload::default();
    for (path, sources) in &summaries[d.function.0 as usize].task_ports {
        b.spend(path.len().saturating_add(sources.len()))?;
        let mut permission = Permission::default();
        for source in sources {
            let actual = match source.base {
                HoldRoot::Capture(_) => {
                    substitute_capture(module, f, i, source, d, values, state, b)?.ok_or_else(
                        || err("returned Task capture is outside its exact Task-port namespace"),
                    )?
                }
                _ => instantiate(module, f, source, arguments, values, state, b)?,
            };
            join_permission(&mut permission, &actual, b)?;
        }
        insert(&mut out, path, &permission, b)?;
    }
    Ok(out)
}
fn captured_source_upper(
    f: &TypedFunction,
    path: &[ContractProjection],
    source: &Source,
    b: &mut Budget,
) -> Check<()> {
    let Some(contract) = &f.callable_return_contract else {
        return Ok(());
    };
    let Some((ContractProjection::Capture(capture), tail)) = path.split_first() else {
        return Err(err(
            "returned Task environment port omits its actual Capture selector",
        ));
    };
    let rows = contract
        .capture_sources
        .as_ref()
        .ok_or_else(|| err("returned Task environment source upper is Unknown"))?;
    b.spend(
        path.len().saturating_add(width(source)).saturating_add(
            rows.iter()
                .fold(0usize, |n, row| {
                    n.saturating_add(row.position.path.len()).saturating_add(
                        row.sources.iter().fold(1usize, |n, source| {
                            n.saturating_add(match source {
                                zil_ir::StorageSource::Exact(position) => position.path.len(),
                                _ => 0,
                            })
                            .saturating_add(1)
                        }),
                    )
                })
                .saturating_mul(path.len().saturating_add(width(source)).saturating_add(1)),
        ),
    )?;
    let upper = rows
        .iter()
        .find(|row| {
            row.position.base == StorageBase::Capture(*capture) && row.position.path == tail
        })
        .ok_or_else(|| err("actual returned Task port has no capture source upper"))?;
    let HoldRoot::Parameter(parameter) = source.base else {
        return Err(err(
            "returned Task capture depends on a non-input activation",
        ));
    };
    let mut actual = source
        .leaf
        .iter()
        .copied()
        .map(ContractProjection::Place)
        .collect::<Vec<_>>();
    if let Some(origin) = &source.retained_port {
        let PortOrigin::Task { carrier, port } = origin else {
            return Err(err("Dyn dependency is not a Task source namespace"));
        };
        actual.extend_from_slice(carrier);
        actual.push(ContractProjection::RetainedPort(*port));
    }
    let position = StoragePosition {
        base: StorageBase::Parameter(parameter),
        path: actual,
    };
    if !crate::exact_storage_sources_contain(&upper.sources, &position)? {
        return Err(err(
            "actual returned Task capture source exceeds its declared input upper",
        ));
    }
    Ok(())
}
pub(super) fn summarize(
    module: &TypedModule,
    f: &TypedFunction,
    p: &Payload,
    summary: &mut Summary,
    b: &mut Budget,
) -> Check<()> {
    if !b.stored_task_flow() {
        return Ok(());
    }
    for (mut path, port, _) in carriers::paths(module, &f.return_type, b)? {
        path.push(ContractProjection::RetainedPort(port));
        if enum_address::inactive_path(&p.enum_variants, &f.return_type, &path, b)? {
            continue;
        }
        b.spend(p.task_ports.iter().fold(path.len(), |n, (actual, _)| {
            n.saturating_add(actual.len())
                .saturating_add(path.len())
                .saturating_add(1)
        }))?;
        if p.task_ports
            .iter()
            .find(|(actual, _)| actual == &path)
            .is_none_or(|(_, permission)| permission.sources.is_empty())
        {
            // Bottom while the call fixed point is being reconstructed. A
            // final missing port keeps this actual summary incomplete.
            summary.complete = false;
        }
    }
    for (path, permission) in &p.task_ports {
        let upper = carriers::relation(module, f, StorageBase::Result, path, b)?;
        for source in &permission.sources {
            captured_source_upper(f, path, source, b)?;
            if source.base != HoldRoot::Static && source_relation(module, f, source, b)? != upper {
                return Err(err(
                    "returned Task port exceeds its declared external relation",
                ));
            }
        }
        b.spend(summary.task_ports.len().saturating_add(1))?;
        if let Some((_, row)) = summary.task_ports.iter_mut().find(|(p, _)| p == path) {
            add_sources(row, &permission.sources, b)?;
        } else {
            source_work(&permission.sources, b)?;
            summary
                .task_ports
                .push((path.clone(), permission.sources.clone()));
        }
    }
    Ok(())
}
pub(super) fn transfer(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    state: &mut Frame,
    values: &[Payload],
    summaries: &[Summary],
    sites: &mut dyn_leases::Sites,
    loans: &mut Vec<Loan>,
    b: &mut Budget,
) -> Check<Option<Payload>> {
    if !b.stored_task_flow() || !operation(&i.kind) {
        return Ok(None);
    }
    let get = |id: ValueId| {
        values
            .get(id.0 as usize)
            .ok_or_else(|| err("Task operand outside actual table"))
    };
    let mut out = Payload::default();
    match &i.kind {
        InstructionKind::TaskCreate {
            worker,
            arguments,
            port_bindings,
        }
        | InstructionKind::TaskCreateOwned {
            worker,
            arguments,
            port_bindings,
        } => {
            let target = module
                .functions
                .get(worker.0 as usize)
                .ok_or_else(|| err("Task worker is absent"))?;
            let produced = i
                .result
                .ok_or_else(|| err("Task constructor result is absent"))?;
            out.enum_variants = enum_address::Variants::runtime(vt(f, produced)?, 0, b)?;
            let (_, context) = task_type(vt(f, produced)?)
                .ok_or_else(|| err("Task constructor has no actual retained context"))?;
            b.spend(context.ports.len().saturating_add(port_bindings.len()))?;
            if context.ports.len() != port_bindings.len() {
                return Err(err(
                    "Task constructor bindings omit actual retained context",
                ));
            }
            let mut all_inputs = true;
            for (ordinal, (port, row)) in context.ports.iter().zip(port_bindings).enumerate() {
                let StorageBase::Parameter(index) = row.source.base else {
                    return Err(err("Task port source is not a parameter"));
                };
                if row.port != ordinal as u32 {
                    return Err(err("Task input has a noncanonical retained port ordinal"));
                }
                let input = *arguments
                    .get(index as usize)
                    .ok_or_else(|| err("Task input is absent"))?;
                let actual = inputs::leaf(module, vt(f, input)?, &row.source.path, b)?;
                type_work(&actual, &b.limits.clone(), b)?;
                type_work(&port.leaf, &b.limits.clone(), b)?;
                if actual != port.leaf {
                    return Err(err(
                        "Task retained port differs from its exact complete input occurrence",
                    ));
                }
                let mut permission =
                    inputs::permission(module, f, input, &row.source.path, values, state, b)?;
                permission = dyn_leases::retain(
                    sites,
                    loans,
                    i,
                    dyn_leases::Destination::Result(port_path(
                        &[PlaceProjection::ErrorArm(false)],
                        ordinal as u32,
                    )),
                    dyn_leases::Component::Slot(row.source.clone()),
                    port.mode,
                    &permission,
                    b,
                )?;
                results::retain_dependencies(
                    module,
                    f,
                    i,
                    arguments,
                    values,
                    state,
                    &summaries[target.id.0 as usize],
                    ordinal as u32,
                    &mut permission,
                    sites,
                    loans,
                    b,
                )?;
                all_inputs &= !permission.sources.is_empty()
                    || inputs::inactive(f, input, &row.source.path, values, b)?
                    || inputs::empty_opaque(module, f, input, &row.source.path, values, state, b)?;
                insert(
                    &mut out,
                    &port_path(&[PlaceProjection::ErrorArm(false)], ordinal as u32),
                    &permission,
                    b,
                )?;
            }
            let returns = results::check_worker(
                module,
                target,
                vt(f, produced)?,
                port_bindings,
                &summaries[target.id.0 as usize],
                b,
            )?;
            out.creation_complete = Some(all_inputs && returns);
        }
        InstructionKind::PlaceTagLocal { local, path }
            if shape(&f.locals[local.0 as usize].ty) && path.is_empty() => {}
        InstructionKind::TakeVariantField { local, path }
            if shape(&f.locals[local.0 as usize].ty) =>
        {
            let prefix = path
                .iter()
                .copied()
                .map(ContractProjection::Place)
                .collect::<Vec<_>>();
            let holder = &mut state.locals[local.0 as usize];
            let mut keep = Vec::new();
            for (key, p) in std::mem::take(&mut holder.task_ports) {
                if key.starts_with(&prefix) {
                    insert(&mut out, &key[prefix.len()..], &p, b)?;
                } else {
                    keep.push((key, p));
                }
            }
            holder.task_ports = keep;
            out.creation_complete = holder.creation_complete;
        }
        InstructionKind::TakeErrorUnion { value, is_error } if shape(vt(f, *value)?) => {
            let prefix = ContractProjection::Place(PlaceProjection::ErrorArm(*is_error));
            for (path, p) in &get(*value)?.task_ports {
                if path.first() == Some(&prefix) {
                    insert(&mut out, &path[1..], p, b)?;
                }
            }
            out.creation_complete = get(*value)?.creation_complete;
        }
        InstructionKind::DropErrorUnion { local } | InstructionKind::DropEnum { local } => {
            state.locals[local.0 as usize] = Payload::default();
        }
        InstructionKind::TaskPoll { task } if shape(vt(f, *task)?) => {
            let outcome = i
                .result
                .map(|id| vt(f, id))
                .transpose()?
                .ok_or_else(|| err("Task poll lacks its complete actual outcome"))?;
            results::poll(
                module,
                f,
                i,
                vt(f, *task)?,
                outcome,
                get(*task)?,
                &mut out,
                sites,
                loans,
                b,
            )?;
        }
        InstructionKind::TaskPollClose { .. } => {
            let outcome = i
                .result
                .map(|id| vt(f, id))
                .transpose()?
                .ok_or_else(|| err("Task close poll lacks its complete actual outcome"))?;
            out.enum_variants = enum_address::Variants::runtime(outcome, 0, b)?;
        }
        InstructionKind::MakeErrorUnion { value, is_error }
            if shape(vt(
                f,
                i.result.ok_or_else(|| err("Task wrapper result absent"))?,
            )?) =>
        {
            for (path, p) in &get(*value)?.task_ports {
                let mut q = vec![ContractProjection::Place(PlaceProjection::ErrorArm(
                    *is_error,
                ))];
                q.extend(path);
                insert(&mut out, &q, p, b)?;
            }
        }
        InstructionKind::ReadErrorUnion { value, is_error } if shape(vt(f, *value)?) => {
            let prefix = ContractProjection::Place(PlaceProjection::ErrorArm(*is_error));
            for (path, p) in &get(*value)?.task_ports {
                if path.first() == Some(&prefix) {
                    insert(&mut out, &path[1..], p, b)?;
                }
            }
        }
        InstructionKind::ErrorTag { value } if shape(vt(f, *value)?) => {}
        InstructionKind::TaskYield
        | InstructionKind::TaskValid { .. }
        | InstructionKind::TaskRequestCancel { .. }
        | InstructionKind::TaskFinishDestroy { .. } => {}
        _ => return Ok(None),
    }
    work(&out, b)?;
    Ok(Some(out))
}

fn anchors(
    module: &TypedModule,
    f: &TypedFunction,
    current: ValueId,
    p: &Permission,
    loans: &[Loan],
    references: &[(Source, bool)],
    b: &mut Budget,
) -> Check<Vec<TaskLoanAnchor>> {
    b.spend(p.loans.len())?;
    let mut pending = p.loans.clone();
    let mut seen = Vec::new();
    let mut out = Vec::new();
    while let Some(id) = pending.pop() {
        b.spend(seen.len().saturating_add(1))?;
        if seen.contains(&id) {
            continue;
        }
        seen.push(id);
        let loan = loans
            .get(id)
            .ok_or_else(|| err("external parent exceeds checked stored arena"))?;
        if id < f.values.len() && id != current.0 as usize {
            out.push(TaskLoanAnchor::Value(ValueId(id as u32)));
            continue;
        }
        if !loan.parents.is_empty() {
            b.spend(loan.parents.len())?;
            pending.extend(&loan.parents);
            continue;
        }
        if id < f.values.len() {
            // An original local-address producer keeps its own ordinary loan;
            // it has no external parent to bridge into that same definition.
            continue;
        }
        for source in &loan.sources {
            if let Some(index) = reference_cells::input_index(references, source, b)? {
                out.push(TaskLoanAnchor::Reference(index));
                continue;
            }
            let base = match source.base {
                HoldRoot::Parameter(index) => StorageBase::Parameter(index),
                HoldRoot::Capture(index) => StorageBase::Capture(index),
                HoldRoot::Static => continue,
                HoldRoot::Local(_) => {
                    return Err(err("Task source parent lacks actual external anchor"));
                }
            };
            let mut path = source
                .leaf
                .iter()
                .copied()
                .map(ContractProjection::Place)
                .collect::<Vec<_>>();
            if let Some(PortOrigin::Task { carrier, port } | PortOrigin::Dyn { carrier, port }) =
                &source.retained_port
            {
                path.extend_from_slice(carrier);
                path.push(ContractProjection::RetainedPort(*port));
            }
            b.spend(path.len().saturating_add(1))?;
            if let StorageBase::Capture(index) = base {
                // This is the exact seeded external port of this entry, not
                // the environment head or a lifetime upper of its target.
                let definition = f
                    .environment
                    .and_then(|id| module.closures.get(id.0 as usize))
                    .filter(|d| {
                        Some(d.id) == f.environment
                            && d.function == f.id
                            && d.storage_life == f.storage_life
                    })
                    .ok_or_else(|| err("Task source parent has no exact actual closure entry"))?;
                let root = definition
                    .captures
                    .get(index as usize)
                    .ok_or_else(|| err("Task source parent capture is absent"))?;
                let Some(PortOrigin::Task { carrier, port }) = &source.retained_port else {
                    return Err(err("captured Task source parent lacks its exact Task port"));
                };
                if !source.leaf.is_empty()
                    || source.heap_element.is_some()
                    || !source.heap_parents.is_empty()
                {
                    return Err(err("captured Task source parent mixes target namespaces"));
                }
                let Type::Task { retained_life, .. } =
                    carriers::endpoint(module, root, carrier, b)?
                else {
                    return Err(err(
                        "captured Task source parent has another actual endpoint kind",
                    ));
                };
                let mode = retained_life
                    .ports
                    .get(*port as usize)
                    .ok_or_else(|| err("captured Task source parent port is absent"))?
                    .mode;
                if loan.exclusive != (mode == StoredBorrowMode::ExclusiveAddress) {
                    return Err(err(
                        "captured Task source parent changes its actual port mode",
                    ));
                }
                port_relation(module, f, base, &path, b)?;
            }
            out.push(TaskLoanAnchor::Input(zil_ir::StoragePosition {
                base,
                path,
            }));
        }
    }
    b.spend(
        out.len()
            .saturating_mul(out.len().checked_ilog2().map_or(1, |n| n as usize + 1)),
    )?;
    out.sort();
    out.dedup();
    Ok(out)
}
pub(super) fn checked_sources(
    module: &TypedModule,
    f: &TypedFunction,
    s: &Solution,
    b: &mut Budget,
) -> Check<TaskFunctionSources> {
    let mut out = TaskFunctionSources {
        covered: true,
        ..TaskFunctionSources::default()
    };
    out.reference_inputs = reference_cells::inputs(s, b)?;
    b.spend(f.values.len())?;
    for (id, p) in s.values.iter().enumerate() {
        let mut row = TaskValueSources::default();
        permission_work(&p.address, b)?;
        b.spend(p.leaves.len().saturating_add(p.task_ports.len()))?;
        for source in &p.address.sources {
            row.pointer.push(TaskPointerRoot {
                reference: reference_cells::input_index(&out.reference_inputs, source, b)?,
                base: source.base,
                dynamic: matches!(source.retained_port, Some(PortOrigin::Dyn { .. })),
                carrier: match &source.retained_port {
                    Some(PortOrigin::Task { carrier, .. } | PortOrigin::Dyn { carrier, .. }) => {
                        b.spend(carrier.len().saturating_add(source.leaf.len()))?;
                        source
                            .leaf
                            .iter()
                            .copied()
                            .map(ContractProjection::Place)
                            .chain(carrier.iter().cloned())
                            .collect()
                    }
                    _ => Vec::new(),
                },
                port: match &source.retained_port {
                    Some(PortOrigin::Task { port, .. } | PortOrigin::Dyn { port, .. }) => {
                        Some(*port)
                    }
                    None | Some(PortOrigin::Reference { .. }) => None,
                },
            });
            row.public.push(source.base);
        }
        row.pointer_parents = anchors(
            module,
            f,
            ValueId(id as u32),
            &p.address,
            &s.loans,
            &out.reference_inputs,
            b,
        )?;
        for (_, permission) in &p.leaves {
            permission_work(permission, b)?;
            for source in &permission.sources {
                row.public.push(source.base);
            }
            row.payload_parents.extend(anchors(
                module,
                f,
                ValueId(id as u32),
                permission,
                &s.loans,
                &out.reference_inputs,
                b,
            )?);
        }
        for (path, permission) in p
            .task_ports
            .iter()
            .map(|(path, permission)| (path, permission))
            .chain(
                p.dyn_ports
                    .0
                    .iter()
                    .map(|(path, port)| (path, &port.permission)),
            )
        {
            b.spend(path.len().saturating_add(permission.sources.len()))?;
            let mut targets = Vec::new();
            for source in &permission.sources {
                row.public.push(source.base);
                targets.push(TaskPointerRoot {
                    reference: reference_cells::input_index(&out.reference_inputs, source, b)?,
                    base: source.base,
                    dynamic: matches!(source.retained_port, Some(PortOrigin::Dyn { .. })),
                    carrier: match &source.retained_port {
                        Some(
                            PortOrigin::Task { carrier, .. } | PortOrigin::Dyn { carrier, .. },
                        ) => {
                            b.spend(carrier.len().saturating_add(source.leaf.len()))?;
                            source
                                .leaf
                                .iter()
                                .copied()
                                .map(ContractProjection::Place)
                                .chain(carrier.iter().cloned())
                                .collect()
                        }
                        _ => Vec::new(),
                    },
                    port: match &source.retained_port {
                        Some(PortOrigin::Task { port, .. } | PortOrigin::Dyn { port, .. }) => {
                            Some(*port)
                        }
                        None | Some(PortOrigin::Reference { .. }) => None,
                    },
                });
            }
            b.spend(
                targets
                    .len()
                    .saturating_mul(targets.len().checked_ilog2().map_or(1, |n| n as usize + 1)),
            )?;
            targets.sort();
            targets.dedup();
            row.retained.push((path.clone(), targets));
            row.payload_parents.extend(anchors(
                module,
                f,
                ValueId(id as u32),
                permission,
                &s.loans,
                &out.reference_inputs,
                b,
            )?);
        }
        value_transport::visit_fields(p, b, |permission, b| {
            permission_work(permission, b)?;
            // Field views/descriptions retain ancestry; they do not create a
            // new Task retained data port or pointer namespace.
            row.payload_parents.extend(anchors(
                module,
                f,
                ValueId(id as u32),
                permission,
                &s.loans,
                &out.reference_inputs,
                b,
            )?);
            Ok(())
        })?;
        for (path, facts) in &p.field_facts {
            b.spend(path.len().saturating_add(facts.address.sources.len()))?;
            row.public
                .extend(facts.address.sources.iter().map(|source| source.base));
        }
        for count in [
            row.pointer.len(),
            row.public.len(),
            row.payload_parents.len(),
        ] {
            b.spend(count.saturating_mul(count.checked_ilog2().map_or(1, |n| n as usize + 1)))?;
        }
        // Keep actual pointer/parent facts for descriptor effects; do not
        // promote an ordinary resource description to a strong public loan.
        if matches!(&f.values[id], Type::Desc(payload)
            if !matches!(payload.as_ref(), Type::ByteBuffer | Type::Task { .. }
                | Type::TaskI32 | Type::OwnedStore { .. } | Type::Mutex { .. }))
        {
            row.public.clear();
        }
        row.pointer.sort();
        row.pointer.dedup();
        row.public.sort();
        row.public.dedup();
        row.payload_parents.sort();
        row.payload_parents.dedup();
        out.values.push(row);
    }
    Ok(out)
}

pub(super) fn wrapper_instruction(
    f: &TypedFunction,
    i: &Instruction,
    b: &mut Budget,
) -> Check<bool> {
    let (source, arm, make) = match i.kind {
        InstructionKind::PlaceTagLocal { local, ref path } => {
            let ty = &f
                .locals
                .get(local.0 as usize)
                .ok_or_else(|| err("Task local tag source absent"))?
                .ty;
            if !shape(ty) {
                return Ok(false);
            }
            if !path.is_empty()
                || !matches!(ty, Type::ErrorUnion { .. })
                || i.result.and_then(|id| f.values.get(id.0 as usize)) != Some(&Type::I32)
            {
                return Err(err("Task local tag lacks exact union root/I32 result"));
            }
            b.spend(1)?;
            return Ok(true);
        }
        InstructionKind::ErrorTag { value } => {
            let ty = vt(f, value)?;
            if !shape(ty) {
                return Ok(false);
            }
            if !matches!(ty, Type::ErrorUnion { .. })
                || i.result.and_then(|id| f.values.get(id.0 as usize)) != Some(&Type::Bool)
            {
                return Err(err("Task error test lacks exact Bool union operation"));
            }
            return Ok(true);
        }
        InstructionKind::MakeErrorUnion { value, is_error } => (value, is_error, true),
        InstructionKind::ReadErrorUnion { value, is_error }
        | InstructionKind::TakeErrorUnion { value, is_error } => (value, is_error, false),
        _ => return Ok(false),
    };
    let result = i
        .result
        .ok_or_else(|| err("Task wrapper has no actual result"))?;
    let ty = vt(f, if make { result } else { source })?;
    if !shape(ty) {
        return Ok(false);
    }
    let Type::ErrorUnion {
        error,
        result: success,
    } = ty
    else {
        return Err(err("Task wrapper operand lacks actual ErrorUnion"));
    };
    let wanted = if arm { error } else { success };
    let actual = vt(f, if make { source } else { result })?;
    type_work(wanted, &b.limits.clone(), b)?;
    type_work(actual, &b.limits.clone(), b)?;
    if actual != wanted.as_ref() {
        return Err(err("Task wrapper differs from actual selected arm type"));
    }
    Ok(true)
}

#[cfg(test)]
mod captured_port_tests {
    use super::*;
    fn fixture() -> (TypedModule, TypedFunction) {
        let (mut module, _) = crate::tests::baseline();
        let mut caller = module.functions[0].clone();
        let leaf = Type::Address {
            mutable: false,
            inner: Box::new(Type::I32),
        };
        let task = Type::Task {
            result: Box::new(Type::I32),
            retained_life: RetainedLifeShape {
                binder_count: 1,
                ports: vec![RetainedBorrowPort {
                    leaf,
                    mode: StoredBorrowMode::SharedAddress,
                    binder: 0,
                }],
                retained: vec![StorageRetainedBinding {
                    path: vec![LifeTypeProjection::Parameter(0)],
                    class: 0,
                    binder: 0,
                }],
                nominal_arguments: vec![],
            },
        };
        let header = StorageLifeContract {
            fixed_binder_count: 1,
            invocation_binder_count: 0,
            bindings: vec![StorageLifeBinding {
                position: StoragePosition {
                    base: StorageBase::Capture(0),
                    path: vec![ContractProjection::RetainedPort(0)],
                },
                binder: StorageBinder::Fixed(0),
            }],
            classes: vec![SignatureLifeClassBinding {
                position: SignatureLifeClass {
                    base: StorageBase::Capture(0),
                    path: vec![LifeTypeProjection::Pointee],
                    class: 0,
                },
                binder: StorageBinder::Fixed(0),
            }],
            static_bindings: vec![],
        };
        module.closures.push(ClosureDefinition {
            id: ClosureId(0),
            function: FunctionId(0),
            captures: vec![Type::Managed(Box::new(task))],
            parameters: vec![],
            result: Type::I32,
            kind: CallableKind::FnOnce,
            storage_life: Some(header.clone()),
            lending_captures: vec![],
        });
        module.functions[0].environment = Some(ClosureId(0));
        module.functions[0].storage_life = Some(header);
        caller.id = FunctionId(1);
        caller.environment = None;
        caller.values = vec![
            Type::Closure(ClosureId(0)),
            Type::Closure(ClosureId(0)),
            Type::Address {
                mutable: false,
                inner: Box::new(Type::Closure(ClosureId(0))),
            },
        ];
        (module, caller)
    }
    fn source(local: u32) -> Source {
        Source {
            base: HoldRoot::Local(LocalId(local)),
            retained_port: None,
            leaf: vec![],
            inline: vec![],
            heap_parents: vec![],
            heap_element: None,
            static_dependency: StaticProvenance::NotStatic,
        }
    }
    fn payload(local: u32, loans: Vec<usize>) -> Payload {
        let mut value = Payload::default();
        value.task_ports.push((
            vec![
                ContractProjection::Capture(0),
                ContractProjection::RetainedPort(0),
            ],
            Permission {
                sources: vec![source(local)],
                loans,
            },
        ));
        value
    }
    #[test]
    fn captured_task_kind_selects_its_own_ledger_and_complete_actual_entry() {
        let (module, _) = fixture();
        let mut b = Budget::new(&Limits::default());
        b.task_retained_environments = true;
        let position = StoragePosition {
            base: StorageBase::Capture(0),
            path: vec![ContractProjection::RetainedPort(0)],
        };
        let leaf = Type::Address {
            mutable: false,
            inner: Box::new(Type::I32),
        };
        assert!(environment_task_leaf(&module, ClosureId(0), &position, &leaf, &mut b).unwrap());
        assert!(
            returned_environment::leaves(&module, ClosureId(0), &mut b)
                .unwrap()
                .is_empty(),
            "logical Task data is never a physical capture-slot GEP"
        );
        assert!(
            b.pending_dyn_captures.is_empty(),
            "Task obligations cannot register an erased Dyn endpoint"
        );
        assert!(
            environment_task_leaf(&module, ClosureId(0), &position, &Type::Bool, &mut b).is_err()
        );
        let mut wrong = position.clone();
        wrong.path[0] = ContractProjection::RetainedPort(1);
        assert!(environment_task_leaf(&module, ClosureId(0), &wrong, &leaf, &mut b).is_err());
        let mut cold = Budget::new(&Limits::default());
        assert!(environment_task_leaf(&module, ClosureId(0), &position, &leaf, &mut cold).is_err());
        let mut other = module.clone();
        other.closures[0].captures[0] = Type::OwnedDyn {
            interface: DynInterfaceId {
                module: zil_base::ModuleId(0),
                index: 99,
            },
        };
        assert!(
            !environment_task_leaf(&other, ClosureId(0), &position, &leaf, &mut cold).unwrap(),
            "actual endpoint kind is selected before any Task feature state"
        );
    }
    #[test]
    fn captured_parent_anchors_require_the_exact_actual_port_and_keep_all_parents() {
        let (module, _) = fixture();
        let mut f = module.functions[0].clone();
        f.values = vec![Type::I32; 3];
        let mut b = Budget::new(&Limits::default());
        b.task_retained_environments = true;
        // These are genuine external leaves seeded from this entry's actual
        // capture Type, not a candidate source list or a Scope upper.
        let mut loans = vec![Loan::default(); f.values.len()];
        let mut captured = Payload::default();
        seed(
            &module,
            &module.closures[0].captures[0],
            HoldRoot::Capture(0),
            &mut captured,
            &mut loans,
            &mut b,
        )
        .unwrap();
        let permission = captured.task_ports[0].1.clone();
        let external = permission.loans[0];
        let expected = TaskLoanAnchor::Input(StoragePosition {
            base: StorageBase::Capture(0),
            path: vec![ContractProjection::RetainedPort(0)],
        });
        assert_eq!(
            anchors(&module, &f, ValueId(0), &permission, &loans, &[], &mut b).unwrap(),
            vec![expected.clone()]
        );
        let indirect = loans.len();
        loans.push(Loan {
            parents: vec![external, 1],
            ..Loan::default()
        });
        let both = Permission {
            sources: vec![],
            loans: vec![indirect],
        };
        assert_eq!(
            anchors(&module, &f, ValueId(0), &both, &loans, &[], &mut b).unwrap(),
            vec![TaskLoanAnchor::Value(ValueId(1)), expected]
        );
        let mut wrong = loans.clone();
        wrong[external].sources[0].base = HoldRoot::Capture(1);
        assert!(anchors(&module, &f, ValueId(0), &permission, &wrong, &[], &mut b).is_err());
        wrong = loans.clone();
        wrong[external].sources[0].retained_port = Some(PortOrigin::Task {
            carrier: vec![],
            port: 1,
        });
        assert!(anchors(&module, &f, ValueId(0), &permission, &wrong, &[], &mut b).is_err());
        wrong = loans.clone();
        wrong[external].exclusive = true;
        assert!(anchors(&module, &f, ValueId(0), &permission, &wrong, &[], &mut b).is_err());
        wrong = loans.clone();
        wrong[external].sources[0].base = HoldRoot::Local(LocalId(0));
        assert!(anchors(&module, &f, ValueId(0), &permission, &wrong, &[], &mut b).is_err());
        let mut foreign = f.clone();
        foreign.id = FunctionId(9);
        assert!(
            anchors(
                &module,
                &foreign,
                ValueId(0),
                &permission,
                &loans,
                &[],
                &mut b
            )
            .is_err()
        );
        let mut no_row = module.clone();
        no_row.functions[0].values = f.values.clone();
        no_row.functions[0]
            .storage_life
            .as_mut()
            .unwrap()
            .bindings
            .clear();
        no_row.closures[0].storage_life = no_row.functions[0].storage_life.clone();
        assert!(
            anchors(
                &no_row,
                &no_row.functions[0],
                ValueId(0),
                &permission,
                &loans,
                &[],
                &mut b
            )
            .is_err()
        );
    }
    #[test]
    fn captured_port_substitution_keeps_exact_receiver_and_all_current_parent_loans() {
        let (module, f) = fixture();
        let mut b = Budget::new(&Limits::default());
        b.task_retained_environments = true;
        let mut source = source(0);
        source.base = HoldRoot::Capture(0);
        source.retained_port = Some(PortOrigin::Task {
            carrier: vec![],
            port: 0,
        });
        let mut state = Frame::default();
        state.locals = vec![Payload::default(); 3];
        state.locals[2] = payload(1, vec![4, 8]);
        let mut borrowed = payload(0, vec![3, 7]);
        borrowed.address = Permission {
            sources: vec![self::source(2)],
            loans: vec![99],
        };
        let values = vec![payload(0, vec![3, 7]), payload(1, vec![4, 8]), borrowed];
        let span = f.span;
        let call = Instruction {
            result: None,
            kind: InstructionKind::CallClosureOnce {
                callee: ValueId(1),
                arguments: vec![],
            },
            span,
        };
        let actual = substitute_capture(
            &module,
            &f,
            &call,
            &source,
            &module.closures[0],
            &values,
            &state,
            &mut b,
        )
        .unwrap()
        .unwrap();
        assert_eq!(actual.sources, vec![self::source(1)]);
        assert_eq!(actual.loans, vec![4, 8]);
        // This isolates the current-holder selection; ordinary callable kind,
        // source, receiver-head and cleanup proofs remain separate obligations.
        let actual = select_actual(
            &module,
            &f,
            ValueId(2),
            &values[2],
            &state,
            &[ContractProjection::Capture(0)],
            0,
            &mut b,
        )
        .unwrap();
        assert_eq!(actual.sources, vec![self::source(1)]);
        assert_eq!(actual.loans, vec![4, 8]);
        let mut wrong = source.clone();
        wrong.retained_port = Some(PortOrigin::Task {
            carrier: vec![ContractProjection::Place(PlaceProjection::Field(0))],
            port: 0,
        });
        assert!(
            substitute_capture(
                &module,
                &f,
                &call,
                &wrong,
                &module.closures[0],
                &values,
                &state,
                &mut b
            )
            .is_err()
        );
        wrong.retained_port = Some(PortOrigin::Dyn {
            carrier: vec![],
            port: 0,
        });
        assert!(
            dyn_captures::substitute(
                &module,
                &f,
                &call,
                &wrong,
                &module.closures[0],
                &[],
                &values,
                &state,
                &mut b
            )
            .is_err()
        );
    }
    #[test]
    fn selecting_a_nested_task_does_not_exchange_equal_port_ordinals() {
        let mut b = Budget::new(&zil_base::Limits::default());
        let mut value = Payload::default();
        for capture in [0, 1] {
            value.task_ports.push((
                vec![
                    ContractProjection::Capture(capture),
                    ContractProjection::RetainedPort(0),
                ],
                Permission {
                    sources: vec![Source {
                        static_dependency: StaticProvenance::NotStatic,
                        base: HoldRoot::Local(LocalId(capture)),
                        retained_port: None,
                        leaf: vec![],
                        inline: vec![],
                        heap_parents: vec![],
                        heap_element: None,
                    }],
                    loans: vec![capture as usize + 4, capture as usize + 8],
                },
            ));
        }
        let actual =
            select_carrier(&value, &[], &[ContractProjection::Capture(0)], 0, &mut b).unwrap();
        assert_eq!(
            actual.sources.iter().map(|s| s.base).collect::<Vec<_>>(),
            vec![HoldRoot::Local(LocalId(0))]
        );
        assert_eq!(actual.loans, vec![4, 8]);
        assert!(
            select_carrier(&value, &[], &[ContractProjection::Capture(2)], 0, &mut b)
                .unwrap()
                .sources
                .is_empty()
        );
    }
}
