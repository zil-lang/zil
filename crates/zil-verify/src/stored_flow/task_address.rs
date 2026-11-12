//! Independent external Task-result permission reconstruction.
use super::*;
mod carriers;

pub(super) fn port_relation(
    module: &TypedModule,
    f: &TypedFunction,
    base: StorageBase,
    path: &[ContractProjection],
    b: &mut Budget,
) -> Check<StorageBinder> {
    carriers::relation(module, f, base, path, b)
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
    let (index, describe) = match i.kind {
        InstructionKind::TakeCapture { index } => (index, false),
        InstructionKind::DescribeCapture { index } => (index, true),
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
    if !b.task_sources() {
        return Ok(());
    }
    b.spend(p.task_ports.len())?;
    for (path, permission) in &p.task_ports {
        b.spend(path.len())?;
        permission_work(permission, b)?;
    }
    Ok(())
}
fn insert(
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
    if !b.task_sources() {
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
    if !b.task_sources() {
        return Ok(());
    }
    for (carrier, port, mode) in carriers::paths(module, ty, b)? {
        b.spend(carrier.len().saturating_mul(3).saturating_add(3))?;
        let source = Source {
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
    if !b.task_sources() {
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
    if !b.task_sources() {
        return Err(err(
            "opaque Task environment leaf has no actual Task consumer",
        ));
    }
    let StorageBase::Capture(index) = position.base else {
        return Err(err("Task environment port lacks real capture"));
    };
    b.spend(carrier.len().saturating_add(1))?;
    let mut complete = vec![ContractProjection::Capture(index)];
    complete.extend_from_slice(carrier);
    let root = Type::Closure(id);
    let endpoint = carriers::endpoint(module, &root, &complete, b)?;
    let Type::Task { retained_life, .. } = endpoint else {
        return Err(err("opaque environment port endpoint is not a Task"));
    };
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
    if !b.task_sources() {
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
        Some(PortOrigin::Dyn { .. }) => {
            Err(err("Dyn port cannot use a Task result source relation"))
        }
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
                HoldRoot::Capture(index) => {
                    let Some(PortOrigin::Task { carrier, port }) = &source.retained_port else {
                        return Err(err(
                            "returned Task capture is outside the Task-port namespace",
                        ));
                    };
                    if !source.inline.is_empty()
                        || source.heap_element.is_some()
                        || !source.heap_parents.is_empty()
                    {
                        return Err(err(
                            "Task capture projection needs its actual data-position consumer",
                        ));
                    }
                    b.spend(
                        source
                            .leaf
                            .len()
                            .saturating_add(carrier.len())
                            .saturating_add(2),
                    )?;
                    let mut wanted = vec![ContractProjection::Capture(index)];
                    wanted.extend(source.leaf.iter().copied().map(ContractProjection::Place));
                    wanted.extend_from_slice(carrier);
                    wanted.push(ContractProjection::RetainedPort(*port));
                    let receiver = &values[callee.0 as usize];
                    b.spend(amount(
                        receiver.task_ports.len().saturating_add(1),
                        wanted.len().saturating_add(1),
                    )?)?;
                    match receiver.task_ports.iter().find(|(key, _)| *key == wanted) {
                        Some((_, actual)) => {
                            permission_work(actual, b)?;
                            actual.clone()
                        }
                        None => Permission::default(),
                    }
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
    if !b.task_sources() {
        return Ok(());
    }
    for (mut path, port, _) in carriers::paths(module, &f.return_type, b)? {
        path.push(ContractProjection::RetainedPort(port));
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
    b: &mut Budget,
) -> Check<Option<Payload>> {
    if !b.task_sources() || !operation(&i.kind) {
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
                if row.port != ordinal as u32 || !row.source.path.is_empty() {
                    return Err(err("Task input has noncanonical port selector"));
                }
                let input = *arguments
                    .get(index as usize)
                    .ok_or_else(|| err("Task input is absent"))?;
                let actual = vt(f, input)?;
                type_work(actual, &b.limits.clone(), b)?;
                type_work(&port.leaf, &b.limits.clone(), b)?;
                if actual != &port.leaf {
                    return Err(err("Task port differs from actual complete argument type"));
                }
                let permission = &get(input)?.address;
                all_inputs &= !permission.sources.is_empty();
                insert(
                    &mut out,
                    &port_path(&[PlaceProjection::ErrorArm(false)], ordinal as u32),
                    permission,
                    b,
                )?;
            }
            out.creation_complete = Some(all_inputs);
            if matches!(target.return_type, Type::Address { mutable: false, .. }) {
                super::super::tasks::charge_result_selection(vt(f, produced)?, b)?;
                b.spend(port_bindings.len())?;
                let mut allowed = Vec::new();
                for row in port_bindings {
                    if super::super::tasks::result_port(vt(f, produced)?, row.port) {
                        allowed.push(row.source.base);
                    }
                }
                let summary = &summaries[target.id.0 as usize];
                let mut present = false;
                for (path, sources) in &summary.result {
                    if path.is_empty() {
                        present |= !sources.is_empty();
                        for source in sources {
                            b.spend(
                                source
                                    .leaf
                                    .len()
                                    .saturating_add(allowed.len())
                                    .saturating_add(source.inline.len())
                                    .saturating_add(1),
                            )?;
                            if !allowed.iter().any(|base| matches!(base, StorageBase::Parameter(index) if source.base == HoldRoot::Parameter(*index)))
                                || source.retained_port.is_some()
                                || !source.leaf.is_empty()
                                || !source.inline.is_empty()
                            {
                                return Err(err(
                                    "Task Ready source differs from its actual retained input",
                                ));
                            }
                        }
                    }
                }
                out.creation_complete = Some(all_inputs && present && summary.complete);
            }
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
            if task_type(vt(f, *task)?)
                .is_some_and(|(result, _)| matches!(result, Type::Address { mutable: false, .. }))
            {
                let (_, context) = task_type(vt(f, *task)?).unwrap();
                super::super::tasks::charge_result_selection(vt(f, *task)?, b)?;
                b.spend(context.ports.len())?;
                let mut p = Permission::default();
                for port in 0..context.ports.len() {
                    if super::super::tasks::result_port(vt(f, *task)?, port as u32) {
                        join_permission(&mut p, &select(get(*task)?, &[], port as u32, b)?, b)?;
                    }
                }
                add_leaf(
                    &mut out,
                    &[
                        PlaceProjection::ErrorArm(false),
                        PlaceProjection::Variant(zil_ir::VariantProjection {
                            variant: 1,
                            index: 0,
                        }),
                    ],
                    &p,
                    b,
                )?;
                out.enum_variants = enum_address::Variants::runtime_poll(b)?;
            }
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
        | InstructionKind::TaskPollClose { .. }
        | InstructionKind::TaskFinishDestroy { .. } => {}
        _ => return Ok(None),
    }
    work(&out, b)?;
    Ok(Some(out))
}

fn anchors(
    f: &TypedFunction,
    current: ValueId,
    p: &Permission,
    loans: &[Loan],
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
            let HoldRoot::Parameter(index) = source.base else {
                if source.base == HoldRoot::Static {
                    continue;
                }
                return Err(err("Task source parent lacks actual external anchor"));
            };
            let mut path = source
                .leaf
                .iter()
                .copied()
                .map(ContractProjection::Place)
                .collect::<Vec<_>>();
            if let Some(PortOrigin::Task { carrier, port }) = &source.retained_port {
                path.extend_from_slice(carrier);
                path.push(ContractProjection::RetainedPort(*port));
            }
            b.spend(path.len().saturating_add(1))?;
            out.push(TaskLoanAnchor::Input(zil_ir::StoragePosition {
                base: StorageBase::Parameter(index),
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
    f: &TypedFunction,
    s: &Solution,
    b: &mut Budget,
) -> Check<TaskFunctionSources> {
    let mut out = TaskFunctionSources {
        covered: true,
        ..TaskFunctionSources::default()
    };
    b.spend(f.values.len())?;
    for (id, p) in s.values.iter().enumerate() {
        let mut row = TaskValueSources::default();
        permission_work(&p.address, b)?;
        b.spend(p.leaves.len().saturating_add(p.task_ports.len()))?;
        for source in &p.address.sources {
            row.pointer.push(TaskPointerRoot {
                base: source.base,
                carrier: match &source.retained_port {
                    Some(PortOrigin::Task { carrier, .. }) => {
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
                    Some(PortOrigin::Task { port, .. }) => Some(*port),
                    Some(PortOrigin::Dyn { .. }) => {
                        return Err(err("Task snapshot cannot erase a Dyn port namespace"));
                    }
                    None => None,
                },
            });
            row.public.push(source.base);
        }
        row.pointer_parents = anchors(f, ValueId(id as u32), &p.address, &s.loans, b)?;
        for (_, permission) in &p.leaves {
            permission_work(permission, b)?;
            for source in &permission.sources {
                row.public.push(source.base);
            }
            row.payload_parents
                .extend(anchors(f, ValueId(id as u32), permission, &s.loans, b)?);
        }
        for (path, permission) in &p.task_ports {
            b.spend(path.len().saturating_add(permission.sources.len()))?;
            let mut targets = Vec::new();
            for source in &permission.sources {
                row.public.push(source.base);
                targets.push(TaskPointerRoot {
                    base: source.base,
                    carrier: match &source.retained_port {
                        Some(PortOrigin::Task { carrier, .. }) => {
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
                        Some(PortOrigin::Task { port, .. }) => Some(*port),
                        Some(PortOrigin::Dyn { .. }) => {
                            return Err(err("Task snapshot cannot erase a Dyn port namespace"));
                        }
                        None => None,
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
