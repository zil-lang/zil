//! Actual head origins and reachable worker graph, never a producer flag.
use super::*;
mod closed_capture;
mod concrete;
mod selected;
#[derive(Clone, Default, PartialEq, Eq)]
struct Origins {
    unknown: bool,
    workers: BTreeSet<FunctionId>,
    parameters: BTreeSet<u32>,
    selected: BTreeSet<zil_ir::StoragePosition>,
}
impl Origins {
    fn add(&mut self, other: &Self, budget: &mut Budget) -> Check<bool> {
        for source in self.selected.iter().chain(&other.selected) {
            budget.spend(
                source.path.len().saturating_add(1).saturating_mul(
                    self.selected
                        .len()
                        .saturating_add(other.selected.len())
                        .saturating_add(1),
                ),
            )?;
        }
        budget.spend(
            self.workers
                .len()
                .saturating_add(other.workers.len())
                .saturating_add(self.parameters.len())
                .saturating_add(other.parameters.len())
                .saturating_add(1),
        )?;
        let old = self.clone();
        self.unknown |= other.unknown;
        self.workers.extend(other.workers.iter().copied());
        self.parameters.extend(other.parameters.iter().copied());
        self.selected.extend(other.selected.iter().cloned());
        Ok(*self != old)
    }
}
pub(super) fn contains_kind(
    module: &TypedModule,
    t: &Type,
    budget: &mut Budget,
    pollable: bool,
) -> Check<bool> {
    resources::charge_payload(t, budget)?;
    let mut pending = vec![t];
    let mut closures = BTreeSet::new();
    while let Some(t) = pending.pop() {
        budget.spend(1)?;
        match t {
            // Legacy TaskI32 has a separately checked NoExternal constructor invariant.
            Type::Task { .. } if !pollable => return Ok(true),
            Type::AsyncRegistration { .. } if pollable => return Ok(true),
            Type::Task { result, .. } => pending.push(result),
            Type::AsyncRegistration { environment } | Type::Registration { environment } => {
                pending.push(environment)
            }
            Type::Managed(p) | Type::Desc(p) | Type::Address { inner: p, .. } | Type::BorrowedStorage { storage:p,.. } | Type::Mutex { payload:p } | Type::MutexGuard { payload:p,.. } => pending.push(p),
            Type::Closure(id) => {
                budget.spend(closures.len().saturating_add(1))?;
                if closures.insert(*id) {
                    let Some(closure) = module.closures.get(id.0 as usize) else {
                        return Ok(true);
                    };
                    for capture in &closure.captures {
                        resources::charge_payload(capture, budget)?;
                        pending.push(capture);
                    }
                }
            }
            Type::Array { element, .. } | Type::OwnedStore { element } => pending.push(element),
            Type::Opaque { hidden, .. } => pending.push(hidden),
            Type::Tuple(ts) => pending.extend(ts),
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                pending.extend(fields.iter().map(|(_, t)| t))
            }
            Type::ErrorUnion { error, result } => {
                pending.push(error);
                pending.push(result);
            }
            Type::Enum { variants, .. } => {
                for (_, ts) in variants {
                    pending.extend(ts);
                }
            }
            _ => {}
        }
    }
    Ok(false)
}
pub(crate) fn needs_proof(module: &TypedModule, t: &Type, budget: &mut Budget) -> Check<bool> {
    if !budget.task_origin_roots {
        return Ok(false);
    }
    contains_kind(module, t, budget, false)
}
fn dependencies(
    module: &TypedModule,
    t: &Type,
    budget: &mut Budget,
    functions: &mut Vec<FunctionId>,
) -> Check<()> {
    resources::charge_payload(t, budget)?;
    let mut pending = vec![t];
    let mut closures = BTreeSet::new();
    while let Some(t) = pending.pop() {
        budget.spend(1)?;
        match t {
            Type::Managed(payload) => {
                for _ in &module.resources {
                    resources::charge_payload(payload, budget)?;
                }
                if let Some(r) = module.resources.iter().find(|r| r.payload == **payload)
                    && let Some(drop) = r.teardown
                {
                    functions.push(drop);
                }
                pending.push(payload);
            }
            Type::Closure(id) => {
                budget.spend(closures.len().saturating_add(1))?;
                if closures.insert(*id)
                    && let Some(c) = module.closures.get(id.0 as usize)
                {
                    functions.push(c.function);
                    for t in &c.captures {
                        resources::charge_payload(t, budget)?;
                        pending.push(t);
                    }
                }
            }
            Type::Desc(p)
            | Type::BorrowedStorage { storage:p,.. }
            | Type::Mutex { payload:p }
            | Type::MutexGuard { payload:p,.. }
            | Type::Address { inner: p, .. }
            | Type::Registration { environment: p }
            | Type::AsyncRegistration { environment: p }
            | Type::Opaque { hidden: p, .. }
            | Type::OwnedStore { element: p }
            | Type::Array { element: p, .. } => pending.push(p),
            Type::Tuple(ts) => pending.extend(ts),
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                pending.extend(fields.iter().map(|(_, t)| t))
            }
            Type::Enum { variants, .. } => {
                for (_, ts) in variants {
                    pending.extend(ts);
                }
            }
            Type::ErrorUnion { error, result } => {
                pending.push(error);
                pending.push(result);
            }
            _ => {}
        }
    }
    Ok(())
}

// A helper returning Task is transparent only after its executable return
// values have been traced to real constructors. The declaration's `create`
// mode is an obligation, not evidence of a worker.
pub(super) fn live_blocks(f: &TypedFunction, budget: &mut Budget) -> Check<Vec<bool>> {
    budget.spend(f.blocks.len().saturating_add(1))?;
    let mut live = vec![false; f.blocks.len()];
    let mut pending = vec![f.entry];
    while let Some(id) = pending.pop() {
        budget.spend(3)?;
        let Some(seen) = live.get_mut(id.0 as usize) else {
            return Err(reject(
                "E-VERIFY-TASK",
                Some(f.span),
                "Task factory CFG target absent",
            ));
        };
        if *seen {
            continue;
        }
        *seen = true;
        match f.blocks[id.0 as usize].terminator {
            Terminator::Jump(next) => pending.push(next),
            Terminator::Branch {
                then_block,
                else_block,
                ..
            } => {
                pending.push(then_block);
                pending.push(else_block);
            }
            _ => {}
        }
    }
    Ok(live)
}

fn returned_workers(
    module: &TypedModule,
    function: FunctionId,
    visiting: &mut BTreeSet<FunctionId>,
    budget: &mut Budget,
) -> Check<Origins> {
    budget.spend(visiting.len().saturating_add(1))?;
    if !visiting.insert(function) {
        return Err(reject(
            "E-VERIFY-TASK",
            None,
            "cyclic Task factory has no complete worker origin",
        ));
    }
    let f = module
        .functions
        .get(function.0 as usize)
        .ok_or_else(|| reject("E-VERIFY-TASK", None, "Task factory target absent"))?;
    if f.foreign.is_some() {
        return Err(reject(
            "E-VERIFY-TASK",
            Some(f.span),
            "foreign Task factory has no actual worker origin",
        ));
    }
    let reachable = live_blocks(f, budget)?;
    let mut origins = Origins::default();
    let mut saw_return = false;
    for (index, block) in f.blocks.iter().enumerate() {
        budget.spend(block.instructions.len().saturating_add(1))?;
        if reachable[index]
            && let Terminator::Return(Some(value)) = &block.terminator
        {
            saw_return = true;
            let actual = returned_value(
                module,
                f,
                &reachable,
                *value,
                visiting,
                &mut BTreeSet::new(),
                &mut BTreeSet::new(),
                budget,
            )?;
            origins.add(&actual, budget)?;
        }
    }
    visiting.remove(&function);
    if !saw_return
        || (origins.workers.is_empty()
            && origins.parameters.is_empty()
            && origins.selected.is_empty())
    {
        origins.unknown = true;
    }
    Ok(origins)
}

#[allow(clippy::too_many_arguments)]
fn returned_value(
    module: &TypedModule,
    f: &TypedFunction,
    reachable: &[bool],
    value: ValueId,
    functions: &mut BTreeSet<FunctionId>,
    values: &mut BTreeSet<ValueId>,
    locals: &mut BTreeSet<LocalId>,
    budget: &mut Budget,
) -> Check<Origins> {
    budget.spend(values.len().saturating_add(1))?;
    if !values.insert(value) {
        return Ok(Origins {
            unknown: true,
            workers: BTreeSet::new(),
            parameters: BTreeSet::new(),
            selected: BTreeSet::new(),
        });
    }
    let mut found = None;
    for (index, block) in f.blocks.iter().enumerate() {
        budget.spend(block.instructions.len().saturating_add(1))?;
        if !reachable[index] {
            continue;
        }
        for instruction in &block.instructions {
            if instruction.result == Some(value) && found.replace(&instruction.kind).is_some() {
                return Err(reject(
                    "E-VERIFY-TASK",
                    Some(instruction.span),
                    "Task factory value has duplicate definitions",
                ));
            }
        }
    }
    let origin = match found {
        Some(InstructionKind::TakeCapture { index }) => {
            let actual = f
                .environment
                .and_then(|id| module.closures.get(id.0 as usize))
                .is_some_and(|d| {
                    d.function == f.id
                        && d.kind == CallableKind::FnOnce
                        && d.captures.get(*index as usize).is_some()
                });
            if !actual {
                return Err(reject(
                    "E-VERIFY-TASK",
                    None,
                    "Task return capture lacks its actual consuming entry",
                ));
            }
            Origins {
                selected: BTreeSet::from([zil_ir::StoragePosition {
                    base: StorageBase::Capture(*index),
                    path: vec![],
                }]),
                ..Origins::default()
            }
        }
        Some(InstructionKind::CallClosureOnce { callee, arguments }) => {
            selected::invocation(module, f, *callee, arguments, functions, budget)?
        }
        Some(InstructionKind::TaskCreateOwned {
            worker, arguments, ..
        }) => {
            let mut result = Origins {
                workers: BTreeSet::from([*worker]),
                ..Origins::default()
            };
            budget.spend(arguments.len())?;
            for argument in arguments {
                let ty = f.values.get(argument.0 as usize).ok_or_else(|| {
                    reject("E-VERIFY-TASK", None, "actual child frame input is absent")
                })?;
                if contains_kind(module, ty, budget, false)? {
                    result.add(
                        &returned_value(
                            module, f, reachable, *argument, functions, values, locals, budget,
                        )?,
                        budget,
                    )?;
                }
            }
            result
        }
        Some(InstructionKind::TaskCreate { worker, .. })
        | Some(InstructionKind::TaskStart { worker, .. }) => Origins {
            unknown: false,
            workers: BTreeSet::from([*worker]),
            parameters: BTreeSet::new(),
            selected: BTreeSet::new(),
        },
        Some(InstructionKind::MakeErrorUnion {
            is_error: true,
            value,
        }) if !contains_kind(
            module,
            f.values
                .get(value.0 as usize)
                .ok_or_else(|| reject("E-VERIFY-TASK", None, "error frame input is absent"))?,
            budget,
            false,
        )? =>
        {
            Origins::default()
        }
        Some(InstructionKind::MakeErrorUnion { value: source, .. })
        | Some(InstructionKind::TakeErrorUnion { value: source, .. })
        | Some(InstructionKind::ReadErrorUnion { value: source, .. }) => returned_value(
            module, f, reachable, *source, functions, values, locals, budget,
        )?,
        Some(InstructionKind::Load(local)) | Some(InstructionKind::TakeLocal { local }) => {
            returned_local(
                module, f, reachable, *local, functions, values, locals, budget,
            )?
        }
        Some(InstructionKind::WithScopedThread { argument, .. }) => returned_value(
            module, f, reachable, *argument, functions, values, locals, budget,
        )?,
        Some(InstructionKind::TakeVariantField { local, .. }) => returned_local(
            module, f, reachable, *local, functions, values, locals, budget,
        )?,
        Some(InstructionKind::MakeClosure {
            captures: fields, ..
        }) => {
            let mut result = Origins::default();
            for field in fields {
                let ty = f.values.get(field.0 as usize).ok_or_else(|| {
                    reject("E-VERIFY-TASK", None, "returned closure Task input absent")
                })?;
                if contains_kind(module, ty, budget, false)? {
                    result.add(
                        &returned_value(
                            module, f, reachable, *field, functions, values, locals, budget,
                        )?,
                        budget,
                    )?;
                }
            }
            result
        }
        Some(InstructionKind::Call {
            function,
            arguments,
        }) => {
            let summary = returned_workers(module, *function, functions, budget)?;
            budget.spend(
                summary
                    .workers
                    .len()
                    .saturating_add(summary.parameters.len()),
            )?;
            let mut actual = Origins {
                unknown: summary.unknown,
                workers: summary.workers,
                parameters: BTreeSet::new(),
                selected: BTreeSet::new(),
            };
            if !summary.selected.is_empty() {
                actual.add(
                    &selected::sources(module, f, arguments, &summary.selected, functions, budget)?,
                    budget,
                )?;
            }
            for parameter in summary.parameters {
                let argument = *arguments.get(parameter as usize).ok_or_else(|| {
                    reject("E-VERIFY-TASK", None, "Task helper result input absent")
                })?;
                actual.add(
                    &returned_value(
                        module, f, reachable, argument, functions, values, locals, budget,
                    )?,
                    budget,
                )?;
            }
            actual
        }
        _ => Origins {
            unknown: true,
            workers: BTreeSet::new(),
            parameters: BTreeSet::new(),
            selected: BTreeSet::new(),
        },
    };
    values.remove(&value);
    Ok(origin)
}

#[allow(clippy::too_many_arguments)]
fn returned_local(
    module: &TypedModule,
    f: &TypedFunction,
    reachable: &[bool],
    local: LocalId,
    functions: &mut BTreeSet<FunctionId>,
    values: &mut BTreeSet<ValueId>,
    locals: &mut BTreeSet<LocalId>,
    budget: &mut Budget,
) -> Check<Origins> {
    budget.spend(
        locals
            .len()
            .saturating_add(f.parameters.len())
            .saturating_add(1),
    )?;
    if let Some(index) = f.parameters.iter().position(|p| *p == local) {
        return Ok(Origins {
            parameters: BTreeSet::from([index as u32]),
            ..Origins::default()
        });
    }
    if !locals.insert(local) {
        return Ok(Origins {
            unknown: true,
            workers: BTreeSet::new(),
            parameters: BTreeSet::new(),
            selected: BTreeSet::new(),
        });
    }
    let mut origin = Origins::default();
    let mut found = false;
    for (index, block) in f.blocks.iter().enumerate() {
        budget.spend(block.instructions.len().saturating_add(1))?;
        if !reachable[index] {
            continue;
        }
        for instruction in &block.instructions {
            if let InstructionKind::UnpackVariant {
                local: source,
                fields,
                ..
            } = &instruction.kind
                && fields.contains(&local)
            {
                found = true;
                origin.add(
                    &returned_local(
                        module, f, reachable, *source, functions, values, locals, budget,
                    )?,
                    budget,
                )?;
            }

            if let InstructionKind::Store {
                local: target,
                value,
            } = &instruction.kind
                && *target == local
            {
                found = true;
                origin.add(
                    &returned_value(
                        module, f, reachable, *value, functions, values, locals, budget,
                    )?,
                    budget,
                )?;
            }
        }
    }
    locals.remove(&local);
    if !found {
        origin.unknown = true;
    }
    Ok(origin)
}
pub(crate) fn require(
    module: &TypedModule,
    f: &TypedFunction,
    local: LocalId,
    at: (usize, usize),
    budget: &mut Budget,
    span: Option<zil_base::Span>,
) -> Check<()> {
    match require_origin(module, f, Some(local), None, budget, span) {
        Ok(()) => Ok(()),
        Err(error) if error.category == zil_base::ResultCategory::ResourceLimit => Err(error),
        Err(error) => {
            if closed_capture::allow_at(module, f, local, at, budget)? {
                Ok(())
            } else {
                Err(error)
            }
        }
    }
}
pub(crate) fn require_value(
    module: &TypedModule,
    f: &TypedFunction,
    value: ValueId,
    budget: &mut Budget,
    span: Option<zil_base::Span>,
) -> Check<()> {
    require_origin(module, f, None, Some(value), budget, span)
}
fn require_origin(
    module: &TypedModule,
    f: &TypedFunction,
    local: Option<LocalId>,
    value: Option<ValueId>,
    budget: &mut Budget,
    span: Option<zil_base::Span>,
) -> Check<()> {
    let ty = if let Some(local) = local {
        &f.locals
            .get(local.0 as usize)
            .ok_or_else(|| reject("E-VERIFY-TASK", span, "Task Drop local absent"))?
            .ty
    } else {
        f.values
            .get(value.expect("value endpoint").0 as usize)
            .ok_or_else(|| reject("E-VERIFY-TASK", span, "Task callback value absent"))?
    };
    // Preflight visits all actual type/capture/resource tables, including unused
    // entries. Without a new Task root there is no new origin obligation. The
    // separate Legacy TaskStart NoExternal constructor proof is never gated here.
    if !budget.task_origin_roots {
        return Ok(());
    }
    if !contains_kind(module, ty, budget, false)? {
        return Ok(());
    }
    budget.spend(
        f.values
            .len()
            .saturating_add(f.locals.len())
            .saturating_add(1),
    )?;
    let mut values = vec![Origins::default(); f.values.len()];
    let mut locals = vec![Origins::default(); f.locals.len()];
    let mut relevant = Vec::with_capacity(f.values.len());
    for t in &f.values {
        relevant.push(contains_kind(module, t, budget, false)?);
    }
    for (index, p) in f.parameters.iter().enumerate() {
        if contains_kind(module, &f.locals[p.0 as usize].ty, budget, false)? {
            budget.spend(1)?;
            locals[p.0 as usize].parameters.insert(index as u32);
        }
    }
    loop {
        let mut changed = false;
        budget.spend(f.blocks.len())?;
        for block in &f.blocks {
            for i in &block.instructions {
                budget.spend(1)?;
                if let InstructionKind::Store { local, value }
                | InstructionKind::StorePackageField { local, value, .. }
                | InstructionKind::StoreOwnedArrayElement { local, value, .. } = i.kind
                {
                    changed |= locals[local.0 as usize].add(&values[value.0 as usize], budget)?;
                }
                if let InstructionKind::UnpackTuple { local, fields }
                | InstructionKind::UnpackVariant { local, fields, .. } = &i.kind
                {
                    for field in fields {
                        budget.spend(locals[local.0 as usize].workers.len().saturating_add(1))?;
                        let origin = locals[local.0 as usize].clone();
                        changed |= locals[field.0 as usize].add(&origin, budget)?;
                    }
                }
                let Some(out) = i.result else { continue };
                if !relevant[out.0 as usize] {
                    continue;
                }
                let origin = match &i.kind {
                    InstructionKind::TaskCreateOwned {
                        worker, arguments, ..
                    } => {
                        let mut origin = Origins {
                            workers: BTreeSet::from([*worker]),
                            ..Origins::default()
                        };
                        budget.spend(arguments.len())?;
                        for argument in arguments {
                            if *relevant.get(argument.0 as usize).ok_or_else(|| {
                                reject(
                                    "E-VERIFY-TASK",
                                    Some(i.span),
                                    "child frame operand is absent",
                                )
                            })? {
                                origin.add(&values[argument.0 as usize], budget)?;
                            }
                        }
                        origin
                    }
                    InstructionKind::TaskCreate { worker, .. }
                    | InstructionKind::TaskStart { worker, .. } => Origins {
                        unknown: false,
                        workers: BTreeSet::from([*worker]),
                        parameters: BTreeSet::new(),
                        selected: BTreeSet::new(),
                    },
                    InstructionKind::WithScopedThread { argument, .. } => {
                        values[argument.0 as usize].clone()
                    }
                    InstructionKind::Call {
                        function,
                        arguments,
                    } => {
                        let summary =
                            returned_workers(module, *function, &mut BTreeSet::new(), budget)?;
                        budget.spend(
                            summary
                                .workers
                                .len()
                                .saturating_add(summary.parameters.len()),
                        )?;
                        let mut actual = Origins {
                            unknown: summary.unknown,
                            workers: summary.workers,
                            parameters: BTreeSet::new(),
                            selected: BTreeSet::new(),
                        };
                        if !summary.selected.is_empty() {
                            actual.add(
                                &selected::sources(
                                    module,
                                    f,
                                    arguments,
                                    &summary.selected,
                                    &mut BTreeSet::new(),
                                    budget,
                                )?,
                                budget,
                            )?;
                        }
                        for parameter in summary.parameters {
                            let argument = *arguments.get(parameter as usize).ok_or_else(|| {
                                reject(
                                    "E-VERIFY-TASK",
                                    Some(i.span),
                                    "Task helper result input absent",
                                )
                            })?;
                            actual.add(&values[argument.0 as usize], budget)?;
                        }
                        actual
                    }
                    InstructionKind::CallClosureOnce { callee, arguments } => selected::invocation(
                        module,
                        f,
                        *callee,
                        arguments,
                        &mut BTreeSet::new(),
                        budget,
                    )?,
                    InstructionKind::Load(local)
                    | InstructionKind::BorrowLocal { local }
                    | InstructionKind::BorrowLocalMut { local }
                    | InstructionKind::TakeLocal { local }
                    | InstructionKind::TakePackageField { local, .. }
                    | InstructionKind::TakeOwnedArrayElement { local, .. }
                    | InstructionKind::TakeVariantField { local, .. }
                    | InstructionKind::TakeTupleField { local, .. }
                    | InstructionKind::BorrowPlaceLocal { local, .. }
                    | InstructionKind::ReadVariantLocal { local, .. } => {
                        budget.spend(locals[local.0 as usize].workers.len().saturating_add(1))?;
                        locals[local.0 as usize].clone()
                    }
                    InstructionKind::ReborrowAddress { address: value, .. }
                    | InstructionKind::DescribeAddress { address: value }
                    | InstructionKind::RegistrationStart {
                        callback: value, ..
                    }
                    | InstructionKind::AsyncRegistrationStart {
                        callback: value, ..
                    }
                    | InstructionKind::ReadErrorUnion { value, .. }
                    | InstructionKind::TakeErrorUnion { value, .. }
                    | InstructionKind::MakeErrorUnion { value, .. }
                    | InstructionKind::ReadVariant { value, .. } => {
                        budget.spend(values[value.0 as usize].workers.len().saturating_add(1))?;
                        values[value.0 as usize].clone()
                    }
                    InstructionKind::ExtractField { aggregate, .. } => {
                        let value = aggregate;
                        budget.spend(values[value.0 as usize].workers.len().saturating_add(1))?;
                        values[value.0 as usize].clone()
                    }
                    InstructionKind::MakeAggregate { fields }
                    | InstructionKind::MakeVariant { fields, .. }
                    | InstructionKind::MakeOwnedTuple { fields }
                    | InstructionKind::MakePackage { fields }
                    | InstructionKind::MakeOwnedArray { elements: fields }
                    | InstructionKind::MakeClosure {
                        captures: fields, ..
                    } => {
                        let mut origin = Origins::default();
                        for value in fields {
                            origin.add(&values[value.0 as usize], budget)?;
                        }
                        origin
                    }
                    _ => Origins {
                        unknown: true,
                        workers: BTreeSet::new(),
                        parameters: BTreeSet::new(),
                        selected: BTreeSet::new(),
                    },
                };
                changed |= values[out.0 as usize].add(&origin, budget)?;
            }
        }
        if !changed {
            break;
        }
    }
    let symbolic = if let Some(local) = local {
        &locals[local.0 as usize]
    } else {
        &values[value.expect("value endpoint").0 as usize]
    };
    let closed;
    let origins = if symbolic.parameters.is_empty() && symbolic.selected.is_empty() {
        symbolic
    } else {
        closed = concrete::settle(module, f.id, symbolic, budget)?;
        &closed
    };
    budget.spend(origins.workers.len().saturating_add(1))?;
    if origins.unknown
        || origins.workers.is_empty()
        || !origins.parameters.is_empty()
        || !origins.selected.is_empty()
    {
        return Err(reject(
            "E-VERIFY-TASK",
            span,
            "Task automatic Drop has no complete synchronous worker origin",
        ));
    }
    require_workers(
        module,
        origins.workers.iter().copied().collect(),
        budget,
        span,
    )
}
pub(crate) fn require_worker(
    module: &TypedModule,
    worker: FunctionId,
    budget: &mut Budget,
    span: Option<zil_base::Span>,
) -> Check<()> {
    require_workers(module, vec![worker], budget, span)
}
fn require_workers(
    module: &TypedModule,
    mut pending: Vec<FunctionId>,
    budget: &mut Budget,
    span: Option<zil_base::Span>,
) -> Check<()> {
    let mut seen = BTreeSet::new();
    while let Some(worker) = pending.pop() {
        budget.spend(seen.len().saturating_add(1))?;
        if !seen.insert(worker) {
            continue;
        }
        let Some(target) = module.functions.get(worker.0 as usize) else {
            return Err(reject(
                "E-VERIFY-TASK",
                span,
                "Task automatic Drop worker absent",
            ));
        };
        for ty in target
            .values
            .iter()
            .chain(target.locals.iter().map(|l| &l.ty))
            .chain(std::iter::once(&target.return_type))
        {
            dependencies(module, ty, budget, &mut pending)?;
            if contains_kind(module, ty, budget, true)? {
                return Err(reject(
                    "E-VERIFY-TASK",
                    span,
                    "pollable Task automatic Drop requires asynchronous continuation",
                ));
            }
        }
        budget.spend(target.blocks.len())?;
        for block in &target.blocks {
            for i in &block.instructions {
                budget.spend(1)?;
                match i.kind {
                    InstructionKind::AsyncRegistrationStart { .. } => {
                        return Err(reject(
                            "E-VERIFY-TASK",
                            span,
                            "pollable Task automatic Drop requires asynchronous continuation",
                        ));
                    }
                    InstructionKind::SuspendCall { function, .. }
                    | InstructionKind::Call { function, .. }
                    | InstructionKind::ForeignCall { function, .. }
                    | InstructionKind::FunctionRef { function }
                    | InstructionKind::ForeignFunctionRef { function }
                    | InstructionKind::TaskCreate {
                        worker: function, ..
                    }
                    | InstructionKind::TaskCreateOwned {
                        worker: function, ..
                    }
                    | InstructionKind::TaskStart {
                        worker: function, ..
                    } => pending.push(function),
                    InstructionKind::CallIndirect { .. } | InstructionKind::CallDynSlot { .. } => {
                        return Err(reject(
                            "E-VERIFY-TASK",
                            span,
                            "synchronous Task close has no finite actual indirect target graph",
                        ));
                    }
                    _ => {}
                }
            }
        }
    }
    Ok(())
}

/// Post-body destruction can use only the actual entry's unconditional facts.
pub(crate) fn once_cleanup_closed(
    module: &TypedModule,
    id: zil_ir::ClosureId,
    b: &mut Budget,
) -> Check<bool> {
    closed_capture::after_once(module, id, b)
}

/// Verify independently searches each normal return backwards. An actual
/// capture move cuts that path; declaration flags never discharge a Task.
pub(crate) fn once_cleanup_has_no_task(
    module: &TypedModule,
    id: zil_ir::ClosureId,
    b: &mut Budget,
) -> Check<bool> {
    if !b.task_origin_roots {
        return Ok(false);
    }
    let Some(d) = module.closures.get(id.0 as usize) else {
        return Ok(false);
    };
    if d.kind != CallableKind::FnOnce {
        return Ok(false);
    }
    let Some(entry) = module.functions.get(d.function.0 as usize) else {
        return Ok(false);
    };
    if entry.environment != Some(id) || entry.foreign.is_some() {
        return Ok(false);
    }
    let mut selected = Vec::new();
    for (index, ty) in d.captures.iter().enumerate() {
        if needs_proof(module, ty, b)? || contains_kind(module, ty, b, true)? {
            selected.push(index as u32);
        }
    }
    if selected.is_empty() {
        return Ok(true);
    }
    b.spend(entry.blocks.len().saturating_mul(3))?;
    let mut predecessors = vec![Vec::new(); entry.blocks.len()];
    let mut reachable = vec![false; entry.blocks.len()];
    let mut frontier = vec![entry.entry.0 as usize];
    while let Some(block) = frontier.pop() {
        b.spend(1)?;
        let seen = reachable.get_mut(block).ok_or_else(|| {
            reject(
                "E-VERIFY-TASK",
                Some(entry.span),
                "Once cleanup entry outside CFG",
            )
        })?;
        if *seen {
            continue;
        }
        *seen = true;
        let targets = match entry.blocks[block].terminator {
            Terminator::Jump(to) => [Some(to), None],
            Terminator::Branch {
                then_block,
                else_block,
                ..
            } => [Some(then_block), Some(else_block)],
            _ => [None, None],
        };
        for target in targets.into_iter().flatten() {
            b.spend(1)?;
            let target = target.0 as usize;
            predecessors
                .get_mut(target)
                .ok_or_else(|| {
                    reject(
                        "E-VERIFY-TASK",
                        Some(entry.span),
                        "Once cleanup successor outside CFG",
                    )
                })?
                .push(block);
            frontier.push(target);
        }
    }
    for capture in selected {
        b.spend(entry.blocks.len())?;
        let mut pending = entry
            .blocks
            .iter()
            .enumerate()
            .filter_map(|(index, block)| {
                (reachable[index] && matches!(block.terminator, Terminator::Return(_)))
                    .then_some(index)
            })
            .collect::<Vec<_>>();
        let mut seen = BTreeSet::new();
        while let Some(block) = pending.pop() {
            b.spend(seen.len().saturating_add(1))?;
            if !seen.insert(block) {
                continue;
            }
            let mut moved = false;
            for instruction in entry.blocks[block].instructions.iter().rev() {
                b.spend(1)?;
                // The nearest event for this actual slot controls its remaining
                // occupant. A refill after an old take does not retire it.
                if matches!(instruction.kind,InstructionKind::WriteCapture{index,..} if index==capture) { return Ok(false); }
                if matches!(instruction.kind,InstructionKind::TakeCapture{index} if index==capture)
                {
                    moved = true;
                    break;
                }
            }
            if moved {
                continue;
            }
            if block == entry.entry.0 as usize {
                return Ok(false);
            }
            b.spend(predecessors[block].len())?;
            pending.extend_from_slice(&predecessors[block]);
        }
    }
    Ok(true)
}

/// One source boundary for an actual environment. Constructor and explicit
/// capture replacements are complete; unmodelled writes through owner aliases
/// invalidate the boundary instead of retaining an old constructor's source.
pub(crate) fn closed_capture_sources(module:&TypedModule,id:ClosureId,index:u32,b:&mut Budget)->Check<Vec<(FunctionId,ValueId)>>{
    let definition=crate::closures::definition(module,id,None)?;
    let ty=definition.captures.get(index as usize).ok_or_else(||reject("E-VERIFY-TASK",None,"environment source capture ordinal is absent"))?;
    check_environment_mutations(module,definition.function,b)?;
    let mut sources=Vec::new();let mut constructor=false;
    for function in &module.functions{for block in &function.blocks{for instruction in &block.instructions{
        b.spend(1)?;
        let value=match &instruction.kind{
            InstructionKind::MakeClosure{closure,captures}if *closure==id=>{
                if captures.len()!=definition.captures.len(){return Err(reject("E-VERIFY-TASK",Some(instruction.span),"environment source changes its full capture arity"));}
                constructor=true;Some(captures[index as usize])
            },
            InstructionKind::WriteCapture{index:actual,value}if function.environment==Some(id)&&*actual==index=>Some(*value),
            _=>None,
        };
        if let Some(value)=value{
            resources::charge_payload(ty,b)?;
            if function.values.get(value.0 as usize)!=Some(ty){return Err(reject("E-VERIFY-TASK",Some(instruction.span),"environment source changes its complete capture type"));}
            if sources.len()>=b.limits.max_nodes{return Err(limit("environment source inventory exceeded"));}
            sources.push((function.id,value));
        }
    }}}
    if !constructor{return Err(reject("E-VERIFY-TASK",None,"environment source lacks its real constructor inventory"));}
    Ok(sources)
}
/// This is a conservative after-source boundary, not a source of permissions.
/// View-local Store/StorageDead never mutates its referent. Physical writes of
/// Task-bearing owners and unresolved call targets require an after-summary.
fn environment_owner_write(kind:&InstructionKind)->Option<ValueId>{
    match kind{
        InstructionKind::WriteAddress{value,..}|InstructionKind::WriteDesc{value,..}|InstructionKind::CommitPlaceReplace{value,..}|InstructionKind::StorePackageField{value,..}|InstructionKind::StoreOwnedArrayElement{value,..}|InstructionKind::OwnedStorePush{value,..}=>Some(*value),
        _=>None,
    }
}
fn check_environment_mutations(module:&TypedModule,entry:FunctionId,b:&mut Budget)->Check<()>{
    let mut pending=vec![entry];let mut seen=BTreeSet::new();
    while let Some(id)=pending.pop(){
        b.spend(seen.len().saturating_add(1))?;if !seen.insert(id){continue;}
        let function=module.functions.get(id.0 as usize).filter(|f|f.id==id).ok_or_else(||reject("E-VERIFY-TASK",None,"environment after-source helper is absent"))?;
        if function.foreign.is_some(){
            for parameter in &function.parameters{
                let ty=&function.locals.get(parameter.0 as usize).ok_or_else(||reject("E-VERIFY-TASK",Some(function.span),"foreign environment effect parameter is absent"))?.ty;
                if contains_kind(module,ty,b,false)?||contains_kind(module,ty,b,true)?{return Err(unsupported("foreign Task owner effect has no complete after-source summary",Some(function.span)));}
            }
        }
        for block in &function.blocks{for instruction in &block.instructions{
            b.spend(1)?;
            let written=environment_owner_write(&instruction.kind);
            if let Some(value)=written{
                let ty=function.values.get(value.0 as usize).ok_or_else(||reject("E-VERIFY-TASK",Some(instruction.span),"environment replacement operand is absent"))?;
                if contains_kind(module,ty,b,false)?||contains_kind(module,ty,b,true)?{return Err(unsupported("environment owner replacement needs its complete after-source summary",Some(instruction.span)));}
            }
            let callable=match &instruction.kind{
                InstructionKind::Call{function,..}|InstructionKind::SuspendCall{function,..}|InstructionKind::ForeignCall{function,..}=>{pending.push(*function);None},
                InstructionKind::CallClosure{callee,..}|InstructionKind::CallClosureOnce{callee,..}|InstructionKind::CallClosureBorrowed{callee,..}=>function.values.get(callee.0 as usize),
                InstructionKind::CallClosureMut{callee,..}|InstructionKind::CallClosureShared{callee,..}=>function.locals.get(callee.0 as usize).map(|l|&l.ty),
                InstructionKind::CallIndirect{..}|InstructionKind::CallDynSlot{..}=>return Err(unsupported("environment mutation has no complete independent indirect after-source summary",Some(instruction.span))),
                _=>None,
            };
            if let Some(mut ty)=callable{
                while let Type::Address{inner,..}|Type::Desc(inner)|Type::Managed(inner)=ty{ty=inner;}
                let Type::Closure(closure)=ty else{return Err(unsupported("environment call has no exact actual closure after-source boundary",Some(instruction.span)));};
                pending.push(crate::closures::definition(module,*closure,Some(instruction.span))?.function);
            }
        }}
    }Ok(())
}

/// A Direct FnOnce capability describes the whole consuming operation, not
/// only its user entry. A surrounding Task continuation cannot supply this
/// synchronous upper. Real constructors and environment writes must all agree.
pub(crate) fn require_direct_consuming_environment(module:&TypedModule,id:ClosureId,b:&mut Budget)->Check<()>{
    let definition=crate::closures::definition(module,id,None)?;
    if definition.captures.iter().map(|ty|Ok(contains_kind(module,ty,b,false)?||contains_kind(module,ty,b,true)?)).collect::<Check<Vec<_>>>()?.into_iter().any(|has|has){check_environment_mutations(module,definition.function,b)?;}
    if once_cleanup_has_no_task(module,id,b)?||once_cleanup_closed(module,id,b)?{return Ok(());}
    // The complete entry/helper execution graph is part of the consuming
    // operation. Safe initial fields alone cannot certify its final occupant.
    require_worker(module,definition.function,b,None)?;
    for(index,ty)in definition.captures.iter().enumerate(){
        if contains_kind(module,ty,b,true)?{return Err(reject("E-VERIFY-TASK",None,"Direct consuming capability has no synchronous remaining asynchronous-registration retirement proof"));}
        if !contains_kind(module,ty,b,false)?{continue;}
        if !b.task_origin_roots{return Err(reject("E-VERIFY-TASK",None,"Direct consuming capability lacks complete Task source scheduling"));}
        for(function,value)in closed_capture_sources(module,id,index as u32,b)?{
            let function=&module.functions[function.0 as usize];
            let source=selected::endpoint(module,function,value,&[],b)?;
            if source.unknown||!source.parameters.is_empty()||!source.selected.is_empty()||source.workers.is_empty(){return Err(reject("E-VERIFY-TASK",None,"Direct consuming capability has no universal synchronous capture source"));}
            require_workers(module,source.workers.into_iter().collect(),b,None)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod direct_consuming_tests{
    use super::*;
    #[test]
    fn direct_consuming_closed_exit_cannot_survive_an_unmodelled_owner_replacement(){
        let limits=Limits::default();let(mut module,_)=crate::tests::baseline();let span=module.functions[0].span;
        let task=Type::Managed(Box::new(Type::Task{result:Box::new(Type::I32),retained_life:super::super::empty_context()}));
        let op=|result:Option<u32>,kind|Instruction{result:result.map(ValueId),kind,span};
        let entry=&mut module.functions[0];entry.environment=Some(ClosureId(0));entry.parameters=vec![LocalId(0)];entry.locals=vec![LocalDecl{id:LocalId(0),name:"new_occupant".into(),ty:task.clone(),mutable:true,span}];
        entry.values=vec![Type::Address{mutable:true,inner:Box::new(task.clone())},super::super::task_poll_type(&Type::Unit),Type::I32,Type::I32,Type::Bool,Type::I32,task.clone()];
        entry.blocks=vec![BasicBlock{id:BlockId(0),instructions:vec![op(Some(0),InstructionKind::BorrowCapture{index:0,mutable:true}),op(Some(1),InstructionKind::TaskPollClose{task:ValueId(0)}),op(Some(2),InstructionKind::VariantTag{value:ValueId(1)}),op(Some(3),InstructionKind::Constant(Constant::I32(1))),op(Some(4),InstructionKind::Binary{op:BinaryOp::Equal,left:ValueId(2),right:ValueId(3)})],terminator:Terminator::Branch{condition:ValueId(4),then_block:BlockId(1),else_block:BlockId(0)}},BasicBlock{id:BlockId(1),instructions:vec![op(Some(5),InstructionKind::Constant(Constant::I32(7)))],terminator:Terminator::Return(Some(ValueId(5)))}];
        module.closures=vec![ClosureDefinition{id:ClosureId(0),function:FunctionId(0),captures:vec![task.clone()],parameters:vec![task],result:Type::I32,kind:CallableKind::FnMut,lending_captures:vec![],storage_life:None}];
        for wire in [false,true]{
            let decoded;let original=if wire{let bytes=zil_ir::wire::encode_module_candidate(&module,&CandidatePlan::empty_for(&module),&limits).unwrap();decoded=zil_ir::wire::decode_module_candidate(&bytes,&limits).unwrap().0;decoded}else{module.clone()};
            let mut b=Budget::new(&limits);b.task_origin_roots=true;assert!(once_cleanup_closed(&original,ClosureId(0),&mut b).unwrap());require_direct_consuming_environment(&original,ClosureId(0),&mut b).unwrap();
            let mut changed=original;changed.functions[0].blocks[1].instructions.extend([op(Some(6),InstructionKind::TakeLocal{local:LocalId(0)}),op(None,InstructionKind::ReservePlaceAddress{reservation:ReservationId(0),address:ValueId(0),path:vec![]}),op(None,InstructionKind::CommitPlaceReplace{reservation:ReservationId(0),value:ValueId(6)})]);
            let mut b=Budget::new(&limits);b.task_origin_roots=true;
            assert!(require_direct_consuming_environment(&changed,ClosureId(0),&mut b).is_err(),"a prior Ready capture proof cannot certify its new live occupant, wire {wire}");
        }
    }
    #[test]
    fn direct_consuming_capability_checks_every_constructor_and_rejects_unknown_replacements_raw_wire(){
        let limits=Limits::default();let(mut module,_)=crate::tests::baseline();let template=module.functions[0].clone();let span=template.span;
        let task=Type::Managed(Box::new(Type::Task{result:Box::new(Type::I32),retained_life:super::super::empty_context()}));
        let outcome=super::super::task_create_type(&Type::I32,super::super::empty_context());
        let op=|result:Option<u32>,kind:InstructionKind|Instruction{result:result.map(ValueId),kind,span};
        let maker=&mut module.functions[0];maker.values=vec![outcome,task.clone(),Type::Closure(ClosureId(0)),task.clone(),Type::Closure(ClosureId(0))];maker.locals=vec![LocalDecl{id:LocalId(0),name:"incoming".into(),ty:task.clone(),mutable:true,span}];maker.parameters=vec![LocalId(0)];maker.blocks=vec![BasicBlock{id:BlockId(0),instructions:vec![op(Some(0),InstructionKind::TaskCreate{worker:FunctionId(1),arguments:vec![],port_bindings:vec![]}),op(Some(1),InstructionKind::TakeErrorUnion{value:ValueId(0),is_error:false}),op(Some(2),InstructionKind::MakeClosure{closure:ClosureId(0),captures:vec![ValueId(1)]})],terminator:Terminator::Return(None)}];
        let mut worker=template.clone();worker.id=FunctionId(1);worker.parameters.clear();worker.locals.clear();worker.values=vec![Type::I32];worker.blocks=vec![BasicBlock{id:BlockId(0),instructions:vec![op(Some(0),InstructionKind::Constant(Constant::I32(7)))],terminator:Terminator::Return(Some(ValueId(0)))}];
        let mut entry=worker.clone();entry.id=FunctionId(2);entry.environment=Some(ClosureId(0));module.functions.extend([worker,entry]);module.closures=vec![ClosureDefinition{id:ClosureId(0),function:FunctionId(2),captures:vec![task],parameters:vec![],result:Type::I32,kind:CallableKind::FnMut,lending_captures:vec![],storage_life:Some(StorageLifeContract{fixed_binder_count:0,invocation_binder_count:0,bindings:vec![],classes:vec![],static_bindings:vec![]})}];
        for wire in [false,true]{
            let plan=CandidatePlan::empty_for(&module);let bytes=zil_ir::wire::encode_module_candidate(&module,&plan,&limits).unwrap();let decoded=zil_ir::wire::decode_module_candidate(&bytes,&limits).unwrap().0;let original=if wire{decoded}else{module.clone()};
            let mut budget=Budget::new(&limits);budget.task_origin_roots=true;require_direct_consuming_environment(&original,ClosureId(0),&mut budget).unwrap();
            let frame_key=crate::semantic::task_domains::FrameKey{worker:FunctionId(1),instance:None,raw_declaration:None};
            let ctor_key=crate::semantic::task_domains::ConstructorKey{caller:None,function:FunctionId(0),block:BlockId(0),instruction:0,result:ValueId(0),worker:FunctionId(1),owned:false};
            let frame=crate::semantic::task_domains::FrameMetadata{key:frame_key,definition:None,arguments:vec![],parameters:vec![],parameter_types:vec![],parameter_lives:vec![],result_life:StorageLifeShape{binder_count:0,slots:vec![],retained:vec![],nominal_arguments:vec![]},result:None,result_type:Type::I32,shape:super::super::empty_context(),port_sources:vec![],input_maps:vec![],result_map:vec![],selectors:vec![],worker_checked:true};
            let mut data=Budget::new(&limits);crate::registration::semantic_closed_environment(&original,ClosureId(0),&mut data).unwrap();assert!(!data.semantic_closed_task_captures.is_empty());
            assert!(crate::registration::finish_semantic_closed_tasks(&original,&mut data).is_err(),"a header/N0 and known worker name cannot discharge missing exact constructor/frame proof");
            data.task_frames.insert(frame_key,crate::semantic::task_domains::FrameState::MetadataReady(frame));
            data.task_constructors.insert(ctor_key,crate::semantic::task_domains::ConstructorObligation{key:ctor_key,ty:original.functions[0].values[0].clone(),frame:Some(frame_key),frames:vec![frame_key],state:crate::semantic::task_domains::ObligationState::Discharged,raw_checked:true,semantic_checked:true});
            crate::registration::finish_semantic_closed_tasks(&original,&mut data).unwrap();assert!(data.semantic_closed_task_captures.is_empty());

            let mut unscheduled=Budget::new(&limits);assert!(require_direct_consuming_environment(&original,ClosureId(0),&mut unscheduled).is_err(),"a missing source pass cannot turn the exact Task field into no obligation");
            let mut changed=original.clone();changed.functions[0].blocks[0].instructions.extend([op(Some(3),InstructionKind::TakeLocal{local:LocalId(0)}),op(Some(4),InstructionKind::MakeClosure{closure:ClosureId(0),captures:vec![ValueId(3)]})]);
            let mut budget=Budget::new(&limits);budget.task_origin_roots=true;assert!(require_direct_consuming_environment(&changed,ClosureId(0),&mut budget).is_err(),"one safe constructor cannot hide a second unknown incoming Task source");
            let mut data=Budget::new(&limits);crate::registration::semantic_closed_environment(&changed,ClosureId(0),&mut data).unwrap();assert!(crate::registration::finish_semantic_closed_tasks(&changed,&mut data).is_err(),"an unrelated same-Type safe Task constructor does not ground an incoming Task capture");

            let mut alias_write=original.clone();let task=alias_write.closures[0].captures[0].clone();
            alias_write.functions[2].values.extend([Type::Address{mutable:true,inner:Box::new(task.clone())},task.clone()]);
            alias_write.functions[2].blocks[0].instructions.push(op(None,InstructionKind::WriteAddress{address:ValueId(1),value:ValueId(2)}));
            let mut budget=Budget::new(&limits);budget.task_origin_roots=true;assert!(require_direct_consuming_environment(&alias_write,ClosureId(0),&mut budget).is_err(),"an unmodelled owner-alias replacement invalidates the safe initial capture");
            let mut data=Budget::new(&limits);crate::registration::semantic_closed_environment(&alias_write,ClosureId(0),&mut data).unwrap();assert!(crate::registration::finish_semantic_closed_tasks(&alias_write,&mut data).is_err(),"CE and Direct consumption use the same actual owner-write boundary");
            let mut view_local=original.clone();let entry=&mut view_local.functions[2];
            entry.locals.push(LocalDecl{id:LocalId(0),name:"temporary_view".into(),ty:Type::Desc(Box::new(Type::I32)),mutable:true,span});
            entry.values.push(Type::Desc(Box::new(Type::I32)));entry.blocks[0].instructions.extend([op(None,InstructionKind::Store{local:LocalId(0),value:ValueId(1)}),op(None,InstructionKind::StorageDead(LocalId(0)))]);
            check_environment_mutations(&view_local,FunctionId(2),&mut Budget::new(&limits)).unwrap();
            let mut replaced=original;let task=replaced.closures[0].captures[0].clone();replaced.closures[0].parameters=vec![task.clone()];replaced.functions[2].locals=vec![LocalDecl{id:LocalId(0),name:"replacement".into(),ty:task.clone(),mutable:true,span}];replaced.functions[2].parameters=vec![LocalId(0)];replaced.functions[2].values.push(task);replaced.functions[2].blocks[0].instructions.extend([op(Some(1),InstructionKind::TakeLocal{local:LocalId(0)}),op(None,InstructionKind::WriteCapture{index:0,value:ValueId(1)})]);
            let mut budget=Budget::new(&limits);budget.task_origin_roots=true;assert!(require_direct_consuming_environment(&replaced,ClosureId(0),&mut budget).is_err(),"a later environment write invalidates an otherwise safe constructor source");
        }
    }
}

/// Select actual hidden frames without applying the synchronous-execution
/// qualification. Callers prove data CE separately from cleanup/NoExternal.
pub(crate) fn closed_frame_workers(module:&TypedModule,function:FunctionId,value:ValueId,path:&[ContractProjection],b:&mut Budget)->Check<BTreeSet<FunctionId>>{
    closed_frame_workers_at(module,function,value,path,&mut BTreeSet::new(),b,0)
}
fn closed_frame_workers_at(module:&TypedModule,function:FunctionId,value:ValueId,path:&[ContractProjection],visiting:&mut BTreeSet<(FunctionId,ValueId,Vec<ContractProjection>)>,b:&mut Budget,depth:usize)->Check<BTreeSet<FunctionId>>{
    if depth>b.limits.max_call_depth.min(128){return Err(limit("closed Task source traversal depth exceeded"));}
    b.spend(visiting.len().saturating_add(path.len()).saturating_add(1))?;
    let key=(function,value,path.to_vec());
    if !visiting.insert(key.clone()){return Err(reject("E-VERIFY-REGISTRATION",None,"Task ClosedEnvironment source has an ungrounded capture cycle"));}
    let result=(||{
    let actual=module.functions.get(function.0 as usize).filter(|row|row.id==function).ok_or_else(||reject("E-VERIFY-TASK",None,"closed frame source function is absent"))?;
    let symbolic=selected::endpoint(module,actual,value,path,b)?;
    let mut sources=symbolic;
    for parameter in std::mem::take(&mut sources.parameters){
        let position=StoragePosition{base:StorageBase::Parameter(parameter),path:vec![]};
        if !crate::dynamic_witness::semantic_patterns::closed_task_input(module,actual,&position,b)?{sources.unknown=true;}
    }
    for position in std::mem::take(&mut sources.selected){
        match position.base{
            StorageBase::Capture(index)=>{
                let closure=actual.environment.ok_or_else(||reject("E-VERIFY-REGISTRATION",None,"closed Task capture source has no exact environment"))?;
                for(creator,operand)in closed_capture_sources(module,closure,index,b)?{
                    sources.workers.extend(closed_frame_workers_at(module,creator,operand,&position.path,visiting,b,depth+1)?);
                }
            },
            StorageBase::Parameter(_)=>if !crate::dynamic_witness::semantic_patterns::closed_task_input(module,actual,&position,b)?{sources.unknown=true;},
            _=>sources.unknown=true,
        }
    }
    b.spend(sources.workers.len().saturating_add(1))?;
    if sources.unknown{return Err(reject("E-VERIFY-REGISTRATION",None,"Task ClosedEnvironment has no complete original input premise or actual constructor source"));}
    Ok(sources.workers)
    })();
    visiting.remove(&key);result
}
