//! Independent external Task-result permission reconstruction.
use super::*;

pub(super) fn shape(ty: &Type) -> bool {
    match ty {
        Type::Task { result, .. } => {
            matches!(result.as_ref(), Type::Address { mutable: false, .. })
        }
        Type::Managed(inner) | Type::Desc(inner) | Type::Address { inner, .. } => shape(inner),
        Type::ErrorUnion { result, .. } => shape(result),
        _ => false,
    }
}
pub(super) fn admitted_value(ty: &Type) -> bool {
    shape(ty)
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
    if !b.task_borrowed_results {
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
    if !b.task_borrowed_results {
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
    if !b.task_borrowed_results || !shape(ty) {
        return Ok(());
    }
    let leaf = wrapper(ty);
    b.spend(leaf.len().saturating_add(3))?;
    let source = Source {
        base,
        leaf: leaf.clone(),
        inline: vec![],
        retained_port: Some(0),
    };
    let id = loans.len();
    loans.push(Loan {
        sources: vec![source.clone()],
        parents: vec![],
        exclusive: false,
    });
    insert(
        out,
        &port_path(&leaf, 0),
        &Permission {
            sources: vec![source],
            loans: vec![id],
        },
        b,
    )?;
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
    summary: &Summary,
    args: &[ValueId],
    values: &[Payload],
    state: &Frame,
    out: &mut Payload,
    b: &mut Budget,
) -> Check<()> {
    if !b.task_borrowed_results {
        return Ok(());
    }
    for (path, sources) in &summary.task_ports {
        let mut p = Permission::default();
        for s in sources {
            join_permission(&mut p, &instantiate(s, args, values, state, b)?, b)?;
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
    if let Some(port) = source.retained_port {
        let c = f
            .storage_life
            .as_ref()
            .ok_or_else(|| err("Task input has no retained relation"))?;
        let p = port_path(&source.leaf, port);
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
    if !b.task_borrowed_results {
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
    if !b.task_borrowed_results || !operation(&i.kind) {
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
        } => {
            let target = module
                .functions
                .get(worker.0 as usize)
                .ok_or_else(|| err("Task worker is absent"))?;
            if !matches!(target.return_type, Type::Address { mutable: false, .. }) {
                return Err(err(
                    "mixed Task result profiles lack their complete retained consumer",
                ));
            }
            if port_bindings.len() != 1 {
                return Err(err("Task borrowed result lacks one exact input port"));
            }
            let row = &port_bindings[0];
            let StorageBase::Parameter(index) = row.source.base else {
                return Err(err("Task port source is not a parameter"));
            };
            if row.port != 0 || !row.source.path.is_empty() {
                return Err(err("Task borrowed result has noncanonical port selector"));
            }
            let input = arguments
                .get(index as usize)
                .ok_or_else(|| err("Task input is absent"))?;
            insert(
                &mut out,
                &port_path(&[PlaceProjection::ErrorArm(false)], 0),
                &get(*input)?.address,
                b,
            )?;
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
                                .saturating_add(source.inline.len())
                                .saturating_add(1),
                        )?;
                        if source.base != HoldRoot::Parameter(index)
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
            out.creation_complete = Some(present && summary.complete);
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
            let p = select(get(*task)?, &[], 0, b)?;
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
    p: &Permission,
    loans: &[Loan],
    b: &mut Budget,
) -> Check<Vec<TaskLoanAnchor>> {
    let mut pending = p.loans.clone();
    let mut seen = Vec::new();
    let mut out = Vec::new();
    while let Some(id) = pending.pop() {
        b.spend(seen.len().saturating_add(1))?;
        if seen.contains(&id) {
            continue;
        }
        seen.push(id);
        if id < f.values.len() {
            out.push(TaskLoanAnchor::Value(ValueId(id as u32)));
            continue;
        }
        let loan = loans
            .get(id)
            .ok_or_else(|| err("external parent exceeds checked stored arena"))?;
        if !loan.parents.is_empty() {
            b.spend(loan.parents.len())?;
            pending.extend(&loan.parents);
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
            if let Some(port) = source.retained_port {
                path.push(ContractProjection::RetainedPort(port));
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
    let mut out = TaskFunctionSources::default();
    b.spend(f.values.len())?;
    for p in &s.values {
        let mut row = TaskValueSources::default();
        for source in &p.address.sources {
            row.pointer.push(source.base);
            row.public.push(source.base);
        }
        row.pointer_parents = anchors(f, &p.address, &s.loans, b)?;
        for (_, permission) in &p.leaves {
            for source in &permission.sources {
                row.public.push(source.base);
            }
            row.payload_parents
                .extend(anchors(f, permission, &s.loans, b)?);
        }
        for (_, permission) in &p.task_ports {
            for source in &permission.sources {
                row.public.push(source.base);
            }
        }
        b.spend(
            row.public.len().saturating_mul(
                row.public
                    .len()
                    .checked_ilog2()
                    .map_or(1, |n| n as usize + 1),
            ),
        )?;
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
