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
            Type::Managed(p) | Type::Desc(p) | Type::Address { inner: p, .. } => pending.push(p),
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
