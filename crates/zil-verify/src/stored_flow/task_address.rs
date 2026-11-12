//! Independent external Task-result permission reconstruction.
use super::*;

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
fn wrapper(ty: &Type) -> Path {
    if let Type::ErrorUnion { result, .. } = ty {
        let mut p = vec![PlaceProjection::ErrorArm(false)];
        p.extend(wrapper(result));
        p
    } else {
        vec![]
    }
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
    ty: &Type,
    base: HoldRoot,
    out: &mut Payload,
    loans: &mut Vec<Loan>,
    b: &mut Budget,
) -> Check<()> {
    if !b.task_sources() || !shape(ty) {
        return Ok(());
    }
    let (_, context) = task_type(ty).unwrap();
    let leaf = wrapper(ty);
    b.spend(context.ports.len().saturating_add(leaf.len()))?;
    for (ordinal, port) in context.ports.iter().enumerate() {
        b.spend(leaf.len().saturating_add(3))?;
        let source = Source {
            base,
            leaf: leaf.clone(),
            inline: vec![],
            heap_parents: Vec::new(),
            heap_element: None,
            retained_port: Some(PortOrigin::Task(ordinal as u32)),
        };
        let id = loans.len();
        loans.push(Loan {
            sources: vec![source.clone()],
            parents: vec![],
            exclusive: port.mode == StoredBorrowMode::ExclusiveAddress,
        });
        b.stored_exclusive_loans |= port.mode == StoredBorrowMode::ExclusiveAddress;
        insert(
            out,
            &port_path(&leaf, ordinal as u32),
            &Permission {
                sources: vec![source],
                loans: vec![id],
            },
            b,
        )?;
    }
    Ok(())
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
    f: &TypedFunction,
    source: &Source,
    b: &mut Budget,
) -> Check<StorageBinder> {
    let HoldRoot::Parameter(index) = source.base else {
        return Err(err("Task result source is activation-local"));
    };
    if let Some(PortOrigin::Task(port)) = &source.retained_port {
        let c = f
            .storage_life
            .as_ref()
            .ok_or_else(|| err("Task input has no retained relation"))?;
        let p = port_path(&source.leaf, *port);
        b.spend(c.bindings.len().saturating_add(p.len()))?;
        c.bindings
            .iter()
            .find(|r| r.position.base == StorageBase::Parameter(index) && r.position.path == p)
            .map(|r| r.binder)
            .ok_or_else(|| err("Task retained input relation omits actual port"))
    } else {
        relation(f, StorageBase::Parameter(index), &source.leaf, b)
    }
}
pub(super) fn summarize(
    f: &TypedFunction,
    p: &Payload,
    summary: &mut Summary,
    b: &mut Budget,
) -> Check<()> {
    if !b.task_sources() {
        return Ok(());
    }
    for (path, permission) in &p.task_ports {
        let c = f
            .storage_life
            .as_ref()
            .ok_or_else(|| err("returned Task lacks complete retained relation"))?;
        b.spend(c.bindings.len().saturating_add(path.len()))?;
        let upper = c
            .bindings
            .iter()
            .find(|r| r.position.base == StorageBase::Result && r.position.path == *path)
            .map(|r| r.binder)
            .ok_or_else(|| err("returned Task omits actual port relation"))?;
        for source in &permission.sources {
            if source.base != HoldRoot::Static && source_relation(f, source, b)? != upper {
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
            if let Some(PortOrigin::Task(port)) = &source.retained_port {
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
                port: match &source.retained_port {
                    Some(PortOrigin::Task(port)) => Some(*port),
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
                    port: match &source.retained_port {
                        Some(PortOrigin::Task(port)) => Some(*port),
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
