//! Task retained-port permissions are independent of the frame's storage.
use super::*;
use zil_ir::ContractProjection;

pub(super) fn shape(ty: &Type) -> Option<&zil_ir::RetainedLifeShape> {
    match ty {
        Type::Task { retained_life, .. } => Some(retained_life),
        Type::Managed(inner) | Type::Desc(inner) | Type::Address { inner, .. } => shape(inner),
        Type::ErrorUnion { result, .. } => shape(result),
        _ => None,
    }
}
fn shared_result(ty: &Type) -> bool {
    match ty {
        Type::Task { result, .. } => {
            matches!(result.as_ref(), Type::Address { mutable: false, .. })
        }
        Type::Managed(inner) | Type::Desc(inner) | Type::Address { inner, .. } => {
            shared_result(inner)
        }
        Type::ErrorUnion { result, .. } => shared_result(result),
        _ => false,
    }
}
pub(super) fn admitted_value(ty: &Type) -> bool {
    shape(ty).is_some()
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
            | InstructionKind::TaskYield
            | InstructionKind::TaskValid { .. }
            | InstructionKind::TaskPoll { .. }
            | InstructionKind::TaskRequestCancel { .. }
            | InstructionKind::TaskPollClose { .. }
            | InstructionKind::TaskFinishDestroy { .. }
            | InstructionKind::ErrorTag { .. }
            | InstructionKind::ReadErrorUnion { .. }
            | InstructionKind::MakeErrorUnion { .. }
    )
}
pub(super) fn work(value: &Value, b: &mut Budget) -> CompileResult<()> {
    if !b.task_sources() {
        return Ok(());
    }
    fee(b, value.task_ports.len())?;
    for (path, permission) in &value.task_ports {
        fee(b, path.len().saturating_add(permission.loans.len()))?;
        target_work(&permission.targets, b)?;
    }
    Ok(())
}
/// The port and its owned paths must be paid before a wrapper copies them.
fn port_snapshot(p: &Permission, path_width: usize, b: &mut Budget) -> CompileResult<Permission> {
    fee(
        b,
        path_width.saturating_add(p.loans.len()).saturating_add(1),
    )?;
    target_work(&p.targets, b)?;
    Ok(p.clone())
}
pub(super) fn merge(to: &mut Value, from: &Value, b: &mut Budget) -> CompileResult<bool> {
    if !b.task_sources() {
        return Ok(false);
    }
    let mut changed = false;
    for (path, permission) in &from.task_ports {
        ordered(b, to.task_ports.len(), path.len())?;
        changed |= union(
            to.task_ports.entry(path.clone()).or_default(),
            permission,
            b,
        )?;
    }
    Ok(changed)
}
fn prefix(ty: &Type) -> Vec<PlaceProjection> {
    match ty {
        Type::ErrorUnion { result, .. } => {
            let mut out = vec![PlaceProjection::ErrorArm(false)];
            out.extend(prefix(result));
            out
        }
        _ => vec![],
    }
}
fn key(path: &[PlaceProjection], port: u32) -> Vec<ContractProjection> {
    path.iter()
        .copied()
        .map(ContractProjection::Place)
        .chain([ContractProjection::RetainedPort(port)])
        .collect()
}
pub(super) fn seed(
    ty: &Type,
    base: HoldRoot,
    value: &mut Value,
    loans: &mut Vec<Loan>,
    b: &mut Budget,
) -> CompileResult<()> {
    if !b.task_sources() || shape(ty).is_none() {
        return Ok(());
    }
    let ports = &shape(ty).unwrap().ports;
    let path = prefix(ty);
    fee(b, ports.len().saturating_add(path.len()))?;
    for (ordinal, port) in ports.iter().enumerate() {
        let target = Target {
            base,
            stored: path.clone(),
            place: vec![],
            heap_parents: Vec::new(),
            heap_element: None,
            retained_port: Some(PortOrigin::Task(ordinal as u32)),
        };
        let id = loans.len();
        fee(b, path.len().saturating_add(3))?;
        loans.push(Loan {
            targets: Targets::from([target.clone()]),
            parents: Loans::new(),
            mutable: port.mode == zil_ir::StoredBorrowMode::ExclusiveAddress,
        });
        b.stored_exclusive_loans |= port.mode == zil_ir::StoredBorrowMode::ExclusiveAddress;
        value.task_ports.insert(
            key(&path, ordinal as u32),
            Permission {
                targets: Targets::from([target]),
                loans: Loans::from([id]),
            },
        );
    }
    Ok(())
}
pub(super) fn select(
    value: &Value,
    path: &[PlaceProjection],
    port: u32,
    b: &mut Budget,
) -> CompileResult<Permission> {
    let key = key(path, port);
    ordered(b, value.task_ports.len(), key.len())?;
    let mut out = Permission::default();
    if let Some(p) = value.task_ports.get(&key) {
        union(&mut out, p, b)?;
    }
    Ok(out)
}
pub(super) fn call(
    summary: &Facts,
    arguments: &[ValueId],
    values: &[Value],
    state: &State,
    out: &mut Value,
    b: &mut Budget,
) -> CompileResult<()> {
    if !b.task_sources() {
        return Ok(());
    }
    for (path, sources) in &summary.returned_task_ports {
        let mut permission = Permission::default();
        for source in sources {
            union(
                &mut permission,
                &substitute(source, arguments, values, state, b)?,
                b,
            )?;
        }
        out.task_ports.insert(path.clone(), permission);
    }
    Ok(())
}
pub(super) fn source_binding(
    f: &TypedFunction,
    source: &Target,
    b: &mut Budget,
) -> CompileResult<StorageBinder> {
    let HoldRoot::Parameter(index) = source.base else {
        return Err(failure("Task result source is not an external input"));
    };
    if let Some(origin) = source.retained_port {
        let PortOrigin::Task(port) = origin else {
            return Err(failure("Dyn dependency cannot use a Task port relation"));
        };
        let c = f
            .storage_life
            .as_ref()
            .ok_or_else(|| failure("Task input omits complete retained relation"))?;
        let path = key(&source.stored, port);
        fee(b, c.bindings.len().saturating_add(path.len()))?;
        c.bindings
            .iter()
            .find(|row| {
                row.position.base == StorageBase::Parameter(index) && row.position.path == path
            })
            .map(|row| row.binder)
            .ok_or_else(|| failure("Task input relation omits actual retained port"))
    } else {
        binding(f, StorageBase::Parameter(index), &source.stored, b)
    }
}
pub(super) fn summarize(
    f: &TypedFunction,
    value: &Value,
    facts: &mut Facts,
    b: &mut Budget,
) -> CompileResult<()> {
    if !b.task_sources() {
        return Ok(());
    }
    for (path, permission) in &value.task_ports {
        let c = f
            .storage_life
            .as_ref()
            .ok_or_else(|| failure("returned Task omits complete retained relation"))?;
        fee(b, c.bindings.len().saturating_add(path.len()))?;
        let upper = c
            .bindings
            .iter()
            .find(|row| row.position.base == StorageBase::Result && row.position.path == *path)
            .map(|row| row.binder)
            .ok_or_else(|| failure("returned Task relation omits actual retained port"))?;
        for source in &permission.targets {
            if source.base != HoldRoot::Static && source_binding(f, source, b)? != upper {
                return Err(failure(
                    "returned Task actual port exceeds its declared relation",
                ));
            }
        }
        let row = facts.returned_task_ports.entry(path.clone()).or_default();
        for source in &permission.targets {
            fee(
                b,
                target_width(source)
                    .saturating_add(1)
                    .saturating_mul(row.len().saturating_add(1)),
            )?;
            if !row.contains(source) {
                row.push(source.clone());
            }
        }
    }
    Ok(())
}
pub(super) fn transfer(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    state: &mut State,
    values: &[Value],
    summaries: &[Facts],
    b: &mut Budget,
) -> CompileResult<Option<Value>> {
    if !b.task_sources() || !operation(&i.kind) {
        return Ok(None);
    }
    let get = |id: ValueId| {
        values
            .get(id.0 as usize)
            .ok_or_else(|| failure("Task operand is absent"))
    };
    let mut out = Value::default();
    let mut work_prepaid = false;
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
            let worker = module
                .functions
                .get(worker.0 as usize)
                .ok_or_else(|| failure("Task worker is absent"))?;
            let result = i
                .result
                .ok_or_else(|| failure("Task constructor has no actual result"))?;
            let ports = &shape(value_type(f, result)?)
                .ok_or_else(|| failure("Task constructor result has no actual retained shape"))?
                .ports;
            fee(b, ports.len().saturating_add(port_bindings.len()))?;
            if ports.len() != port_bindings.len() {
                return Err(failure(
                    "Task constructor bindings omit actual retained ports",
                ));
            }
            let mut complete = true;
            for (ordinal, (port, binding)) in ports.iter().zip(port_bindings).enumerate() {
                let StorageBase::Parameter(index) = binding.source.base else {
                    return Err(failure("Task port is not an actual parameter"));
                };
                if !binding.source.path.is_empty() || binding.port != ordinal as u32 {
                    return Err(failure("Task input port selector is not canonical"));
                }
                let input = *arguments
                    .get(index as usize)
                    .ok_or_else(|| failure("Task input is absent"))?;
                let actual = value_type(f, input)?;
                b.type_work(actual, false, "actual retained Task argument comparison")?;
                b.type_work(&port.leaf, false, "actual retained Task port comparison")?;
                if actual != &port.leaf {
                    return Err(failure("Task port differs from actual full input type"));
                }
                let mut permission = Permission::default();
                union(&mut permission, &get(input)?.pointer, b)?;
                complete &= !permission.targets.is_empty();
                out.task_ports.insert(
                    key(&[PlaceProjection::ErrorArm(false)], ordinal as u32),
                    permission,
                );
            }
            out.creation_complete = Some(complete);
            if matches!(worker.return_type, Type::Address { mutable: false, .. }) {
                super::super::tasks::charge_result_selection(value_type(f, result)?, b)?;
                fee(b, port_bindings.len())?;
                let mut allowed = Vec::new();
                for row in port_bindings {
                    if super::super::tasks::result_port(value_type(f, result)?, row.port) {
                        allowed.push(row.source.base);
                    }
                }
                let summary = &summaries[worker.id.0 as usize];
                let sources = summary.returned.get(&Vec::new());
                if let Some(sources) = sources {
                    for source in sources {
                        fee(
                            b,
                            source
                                .stored
                                .len()
                                .saturating_add(allowed.len())
                                .saturating_add(source.place.len())
                                .saturating_add(1),
                        )?;
                        if !allowed.iter().any(|base| matches!(base, StorageBase::Parameter(index) if source.base == HoldRoot::Parameter(*index)))
                            || source.retained_port.is_some()
                            || !source.stored.is_empty()
                            || !source.place.is_empty()
                        {
                            return Err(failure(
                                "Task Ready result is not the actual retained input port",
                            ));
                        }
                    }
                }
                out.creation_complete = Some(
                    complete && summary.complete && sources.is_some_and(|row| !row.is_empty()),
                );
            }
        }
        InstructionKind::PlaceTagLocal { local, path }
            if shape(&f.locals[local.0 as usize].ty).is_some() && path.is_empty() => {}
        InstructionKind::TakeVariantField { local, path }
            if shape(&f.locals[local.0 as usize].ty).is_some() =>
        {
            let prefix = path
                .iter()
                .copied()
                .map(ContractProjection::Place)
                .collect::<Vec<_>>();
            let holder = state
                .locals
                .get_mut(local)
                .ok_or_else(|| failure("taken Task wrapper is unavailable"))?;
            let mut keep = BTreeMap::new();
            for (key, p) in std::mem::take(&mut holder.task_ports) {
                if key.starts_with(&prefix) {
                    out.task_ports.insert(key[prefix.len()..].to_vec(), p);
                } else {
                    keep.insert(key, p);
                }
            }
            holder.task_ports = keep;
            out.creation_complete = holder.creation_complete;
        }
        InstructionKind::TakeErrorUnion { value, is_error }
            if shape(value_type(f, *value)?).is_some() =>
        {
            work_prepaid = true;
            let prefix = ContractProjection::Place(PlaceProjection::ErrorArm(*is_error));
            for (path, p) in &get(*value)?.task_ports {
                if path.first() == Some(&prefix) {
                    let permission = port_snapshot(p, path.len() - 1, b)?;
                    out.task_ports.insert(path[1..].to_vec(), permission);
                }
            }
            out.creation_complete = get(*value)?.creation_complete;
        }
        InstructionKind::DropErrorUnion { local } | InstructionKind::DropEnum { local } => {
            state.locals.remove(local);
        }
        InstructionKind::TaskPoll { task } if shape(value_type(f, *task)?).is_some() => {
            if shared_result(value_type(f, *task)?) {
                let context = shape(value_type(f, *task)?).unwrap();
                super::super::tasks::charge_result_selection(value_type(f, *task)?, b)?;
                fee(b, context.ports.len())?;
                let mut permission = Permission::default();
                for port in 0..context.ports.len() {
                    if super::super::tasks::result_port(value_type(f, *task)?, port as u32) {
                        union(
                            &mut permission,
                            &select(get(*task)?, &[], port as u32, b)?,
                            b,
                        )?;
                    }
                }
                let path = vec![
                    PlaceProjection::ErrorArm(false),
                    PlaceProjection::Variant(zil_ir::VariantProjection {
                        variant: 1,
                        index: 0,
                    }),
                ];
                out.slots.insert(path, permission);
                // Both runtime alternatives are possible. Only Ready owns a leaf.
                out.enum_variants = enum_address::Variants::runtime_poll(b)?;
            }
        }
        InstructionKind::MakeErrorUnion { value, is_error }
            if shape(value_type(
                f,
                i.result
                    .ok_or_else(|| failure("Task wrapper has no result"))?,
            )?)
            .is_some() =>
        {
            work_prepaid = true;
            for (path, p) in &get(*value)?.task_ports {
                let permission = port_snapshot(p, path.len().saturating_add(1), b)?;
                let mut selected = vec![ContractProjection::Place(PlaceProjection::ErrorArm(
                    *is_error,
                ))];
                selected.extend(path);
                out.task_ports.insert(selected, permission);
            }
        }
        InstructionKind::ReadErrorUnion { value, is_error }
            if shape(value_type(f, *value)?).is_some() =>
        {
            work_prepaid = true;
            let step = ContractProjection::Place(PlaceProjection::ErrorArm(*is_error));
            for (path, p) in &get(*value)?.task_ports {
                if path.first() == Some(&step) {
                    let permission = port_snapshot(p, path.len() - 1, b)?;
                    out.task_ports.insert(path[1..].to_vec(), permission);
                }
            }
        }
        InstructionKind::ErrorTag { value } if shape(value_type(f, *value)?).is_some() => {}
        InstructionKind::TaskYield
        | InstructionKind::TaskValid { .. }
        | InstructionKind::TaskRequestCancel { .. }
        | InstructionKind::TaskPollClose { .. }
        | InstructionKind::TaskFinishDestroy { .. } => {}
        _ => return Ok(None),
    }
    if !work_prepaid {
        work(&out, b)?;
    }
    Ok(Some(out))
}

pub(super) fn wrapper_instruction(
    f: &TypedFunction,
    i: &Instruction,
    b: &mut Budget,
) -> CompileResult<bool> {
    let (source, arm, make) = match i.kind {
        InstructionKind::PlaceTagLocal { local, ref path } => {
            let ty = &f
                .locals
                .get(local.0 as usize)
                .ok_or_else(|| failure("Task tag local is absent"))?
                .ty;
            if shape(ty).is_none() {
                return Ok(false);
            }
            if !path.is_empty()
                || !matches!(ty, Type::ErrorUnion { .. })
                || i.result.and_then(|id| f.values.get(id.0 as usize)) != Some(&Type::I32)
            {
                return Err(failure(
                    "Task local tag lacks exact union root and I32 result",
                ));
            }
            fee(b, 1)?;
            return Ok(true);
        }
        InstructionKind::ErrorTag { value } => {
            let ty = value_type(f, value)?;
            if shape(ty).is_none() {
                return Ok(false);
            }
            if !matches!(ty, Type::ErrorUnion { .. })
                || i.result.and_then(|id| f.values.get(id.0 as usize)) != Some(&Type::Bool)
            {
                return Err(failure("Task error tag lacks actual Bool union operation"));
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
        .ok_or_else(|| failure("Task wrapper operation has no result"))?;
    let ty = value_type(f, if make { result } else { source })?;
    if shape(ty).is_none() {
        return Ok(false);
    }
    let Type::ErrorUnion {
        error,
        result: success,
    } = ty
    else {
        return Err(failure("Task wrapper operation has no actual ErrorUnion"));
    };
    let wanted = if arm { error } else { success };
    let actual = value_type(f, if make { source } else { result })?;
    b.type_work(wanted, false, "actual Task wrapper selected type")?;
    b.type_work(actual, false, "actual Task wrapper operand type")?;
    if actual != wanted.as_ref() {
        return Err(failure("Task wrapper changes actual selected type"));
    }
    Ok(true)
}
