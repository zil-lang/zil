//! Actual head origins and reachable worker graph, never a producer flag.
use super::*;
use std::collections::BTreeMap;
#[derive(Clone, Default, PartialEq, Eq)]
struct Origins {
    unknown: bool,
    // A checked active alternative contains no Task at all.
    vacant: bool,
    workers: BTreeSet<FunctionId>,
    parameters: BTreeSet<u32>,
}
impl Origins {
    fn add(&mut self, other: &Self, budget: &mut Budget) -> CompileResult<bool> {
        budget.spend(
            self.workers
                .len()
                .saturating_add(other.workers.len())
                .saturating_add(self.parameters.len())
                .saturating_add(other.parameters.len())
                .saturating_add(1),
            "Task automatic Drop actual origins",
        )?;
        let old = self.clone();
        self.unknown |= other.unknown;
        self.vacant |= other.vacant;
        self.workers.extend(other.workers.iter().copied());
        self.parameters.extend(other.parameters.iter().copied());
        Ok(*self != old)
    }
}
fn contains_kind(
    module: &TypedModule,
    t: &Type,
    budget: &mut Budget,
    pollable: bool,
) -> CompileResult<bool> {
    budget.type_work(t, false, "Task automatic Drop actual type")?;
    let mut pending = vec![t];
    let mut closures = BTreeSet::new();
    while let Some(t) = pending.pop() {
        budget.spend(1, "Task automatic Drop actual origins")?;
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
                budget.spend(
                    closures.len().saturating_add(1),
                    "Task actual capture roots",
                )?;
                if closures.insert(*id) {
                    let Some(closure) = module.closures.get(id.0 as usize) else {
                        return Ok(true);
                    };
                    for capture in &closure.captures {
                        budget.type_work(capture, false, "Task actual capture root")?;
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
pub(crate) fn needs_proof(
    module: &TypedModule,
    t: &Type,
    budget: &mut Budget,
) -> CompileResult<bool> {
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
) -> CompileResult<()> {
    budget.type_work(t, false, "Task Drop actual dependency type")?;
    let mut pending = vec![t];
    let mut closures = BTreeSet::new();
    while let Some(t) = pending.pop() {
        budget.spend(1, "Task Drop actual dependencies")?;
        match t {
            Type::Managed(payload) => {
                for _ in &module.resources {
                    budget.type_work(payload, false, "Task Drop actual dependency type")?;
                }
                if let Some(r) = module.resources.iter().find(|r| r.payload == **payload)
                    && let Some(drop) = r.teardown
                {
                    functions.push(drop);
                }
                pending.push(payload);
            }
            Type::Closure(id) => {
                budget.spend(
                    closures.len().saturating_add(1),
                    "Task Drop actual dependencies",
                )?;
                if closures.insert(*id)
                    && let Some(c) = module.closures.get(id.0 as usize)
                {
                    functions.push(c.function);
                    for t in &c.captures {
                        budget.type_work(t, false, "Task Drop actual dependency type")?;
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
pub(crate) fn require(
    module: &TypedModule,
    f: &TypedFunction,
    local: LocalId,
    budget: &mut Budget,
    span: Option<zil_base::Span>,
) -> CompileResult<()> {
    require_origin(module, f, Some(local), None, budget, span)
}
pub(crate) fn require_value(
    module: &TypedModule,
    f: &TypedFunction,
    value: ValueId,
    budget: &mut Budget,
    span: Option<zil_base::Span>,
) -> CompileResult<()> {
    require_origin(module, f, None, Some(value), budget, span)
}
fn require_origin(
    module: &TypedModule,
    f: &TypedFunction,
    local: Option<LocalId>,
    value: Option<ValueId>,
    budget: &mut Budget,
    span: Option<zil_base::Span>,
) -> CompileResult<()> {
    let ty = if let Some(local) = local {
        &f.locals
            .get(local.0 as usize)
            .ok_or_else(|| fail(span.unwrap_or(f.span), "Task Drop local absent"))?
            .ty
    } else {
        f.values
            .get(value.expect("value endpoint").0 as usize)
            .ok_or_else(|| fail(span.unwrap_or(f.span), "Task callback value absent"))?
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
    budget.spend(1, "Task factory return root activation")?;
    let mut active = BTreeSet::from([f.id]);
    let mut returned = BTreeMap::new();
    let (values, locals) = infer_origins(module, f, budget, &mut active, &mut returned)?;
    let origins = if let Some(local) = local {
        &locals[local.0 as usize]
    } else {
        &values[value.expect("value endpoint").0 as usize]
    };
    budget.spend(
        origins.workers.len().saturating_add(1),
        "Task Drop actual dependencies",
    )?;
    if origins.unknown || (origins.workers.is_empty() && !origins.vacant) {
        return Err(fail(
            span.unwrap_or(f.span),
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

fn reachable_blocks(f: &TypedFunction, budget: &mut Budget) -> CompileResult<Vec<bool>> {
    budget.spend(
        f.blocks.len().saturating_add(1),
        "Task return actual reachability",
    )?;
    let mut reachable = vec![false; f.blocks.len()];
    let mut pending = vec![f.entry];
    while let Some(block) = pending.pop() {
        budget.spend(3, "Task return actual CFG edge")?;
        let Some(visited) = reachable.get_mut(block.0 as usize) else {
            return Err(fail(f.span, "Task return CFG target absent"));
        };
        if *visited {
            continue;
        }
        *visited = true;
        match f.blocks[block.0 as usize].terminator {
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
    Ok(reachable)
}

fn returned_origin(
    module: &TypedModule,
    id: FunctionId,
    budget: &mut Budget,
    active: &mut BTreeSet<FunctionId>,
    returned: &mut BTreeMap<FunctionId, Origins>,
) -> CompileResult<Origins> {
    budget.spend(
        active
            .len()
            .saturating_add(returned.len())
            .saturating_add(1),
        "Task factory actual return dependencies",
    )?;
    if let Some(origin) = returned.get(&id) {
        budget.spend(
            origin
                .workers
                .len()
                .saturating_add(origin.parameters.len())
                .saturating_add(1),
            "Task factory cached return snapshot",
        )?;
        return Ok(origin.clone());
    }
    let target = module
        .functions
        .get(id.0 as usize)
        .ok_or_else(|| fail(Default::default(), "Task factory target absent"))?;
    if active.contains(&id) {
        return Err(fail(
            target.span,
            "recursive Task factory returns have no finite synchronous worker origin",
        ));
    }
    if active.len() >= budget.helper_depth {
        return Err(vec![Diagnostic::limit(
            Phase::Analyze,
            "Task factory return dependency depth exceeded",
            Some(target.span),
        )]);
    }
    if target.foreign.is_some() {
        return Ok(Origins {
            unknown: true,
            ..Origins::default()
        });
    }
    active.insert(id);
    let solved = infer_origins(module, target, budget, active, returned);
    active.remove(&id);
    let (values, _) = solved?;
    let reachable = reachable_blocks(target, budget)?;
    let mut result = Origins::default();
    let mut has_return = false;
    for (block, live) in target.blocks.iter().zip(reachable) {
        budget.spend(1, "Task factory actual returned carrier")?;
        if !live {
            continue;
        }
        if let Terminator::Return(value) = block.terminator {
            has_return = true;
            if let Some(value) = value {
                budget.type_work(
                    &target.return_type,
                    false,
                    "Task factory actual return signature",
                )?;
                let actual = target
                    .values
                    .get(value.0 as usize)
                    .ok_or_else(|| fail(target.span, "Task factory return value absent"))?;
                budget.type_work(actual, false, "Task factory actual returned value")?;
                if actual != &target.return_type {
                    result.unknown = true;
                }
                let origin = &values[value.0 as usize];
                if !origin.unknown
                    && !origin.vacant
                    && origin.workers.is_empty()
                    && origin.parameters.is_empty()
                {
                    result.unknown = true;
                }
                result.add(origin, budget)?;
            } else {
                result.unknown = true;
            }
        }
    }
    result.unknown |= !has_return;
    budget.spend(
        result
            .workers
            .len()
            .saturating_add(result.parameters.len())
            .saturating_add(1),
        "Task factory return cache insertion",
    )?;
    returned.insert(id, result.clone());
    Ok(result)
}

fn infer_origins(
    module: &TypedModule,
    f: &TypedFunction,
    budget: &mut Budget,
    active: &mut BTreeSet<FunctionId>,
    returned: &mut BTreeMap<FunctionId, Origins>,
) -> CompileResult<(Vec<Origins>, Vec<Origins>)> {
    budget.spend(
        f.values
            .len()
            .saturating_add(f.locals.len())
            .saturating_add(1),
        "Task automatic Drop actual origins",
    )?;
    let mut values = vec![Origins::default(); f.values.len()];
    let mut locals = vec![Origins::default(); f.locals.len()];
    let mut relevant = Vec::with_capacity(f.values.len());
    for t in &f.values {
        relevant.push(contains_kind(module, t, budget, false)?);
    }
    for (index, p) in f.parameters.iter().enumerate() {
        if contains_kind(module, &f.locals[p.0 as usize].ty, budget, false)? {
            budget.spend(1, "Task formal owner source selector")?;
            locals[p.0 as usize].parameters.insert(index as u32);
        }
    }
    let reachable = reachable_blocks(f, budget)?;
    loop {
        let mut changed = false;
        budget.spend(f.blocks.len(), "Task Drop actual graph")?;
        for (index, block) in f.blocks.iter().enumerate() {
            if !reachable[index] {
                continue;
            }
            for i in &block.instructions {
                budget.spend(1, "Task automatic Drop actual origins")?;
                if let InstructionKind::Store { local, value }
                | InstructionKind::StorePackageField { local, value, .. }
                | InstructionKind::StoreOwnedArrayElement { local, value, .. } = i.kind
                {
                    changed |= locals[local.0 as usize].add(&values[value.0 as usize], budget)?;
                }
                if let InstructionKind::UnpackTuple { local, fields } = &i.kind {
                    for field in fields {
                        budget.spend(
                            locals[local.0 as usize].workers.len().saturating_add(1),
                            "Task Drop actual graph",
                        )?;
                        let origin = locals[local.0 as usize].clone();
                        changed |= locals[field.0 as usize].add(&origin, budget)?;
                    }
                }
                let Some(out) = i.result else { continue };
                if !relevant[out.0 as usize] {
                    continue;
                }
                let origin = match &i.kind {
                    InstructionKind::TaskCreate { worker, .. }
                    | InstructionKind::TaskStart { worker, .. } => Origins {
                        unknown: false,
                        vacant: false,
                        workers: BTreeSet::from([*worker]),
                        parameters: BTreeSet::new(),
                    },
                    InstructionKind::Call {
                        function,
                        arguments,
                    } => {
                        let summary = returned_origin(module, *function, budget, active, returned)?;
                        let mut actual = Origins {
                            unknown: summary.unknown,
                            vacant: summary.vacant,
                            workers: summary.workers.clone(),
                            parameters: BTreeSet::new(),
                        };
                        budget.spend(
                            summary
                                .workers
                                .len()
                                .saturating_add(summary.parameters.len()),
                            "Task helper return actual source",
                        )?;
                        for parameter in &summary.parameters {
                            let source = arguments
                                .get(*parameter as usize)
                                .ok_or_else(|| fail(i.span, "Task helper result input absent"))?;
                            actual.add(&values[source.0 as usize], budget)?;
                        }
                        actual
                    }
                    InstructionKind::MakeErrorUnion { value, .. }
                        if !relevant[value.0 as usize] =>
                    {
                        Origins {
                            vacant: true,
                            ..Origins::default()
                        }
                    }
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
                        budget.spend(
                            locals[local.0 as usize].workers.len().saturating_add(1),
                            "Task automatic Drop actual origins",
                        )?;
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
                        budget.spend(
                            values[value.0 as usize].workers.len().saturating_add(1),
                            "Task automatic Drop actual origins",
                        )?;
                        values[value.0 as usize].clone()
                    }
                    InstructionKind::ExtractField { aggregate, .. } => {
                        let value = aggregate;
                        budget.spend(
                            values[value.0 as usize].workers.len().saturating_add(1),
                            "Task automatic Drop actual origins",
                        )?;
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
                        vacant: false,
                        workers: BTreeSet::new(),
                        parameters: BTreeSet::new(),
                    },
                };
                changed |= values[out.0 as usize].add(&origin, budget)?;
            }
        }
        if !changed {
            break;
        }
    }
    Ok((values, locals))
}
pub(crate) fn require_worker(
    module: &TypedModule,
    worker: FunctionId,
    budget: &mut Budget,
    span: Option<zil_base::Span>,
) -> CompileResult<()> {
    require_workers(module, vec![worker], budget, span)
}
fn require_workers(
    module: &TypedModule,
    mut pending: Vec<FunctionId>,
    budget: &mut Budget,
    span: Option<zil_base::Span>,
) -> CompileResult<()> {
    let mut seen = BTreeSet::new();
    while let Some(worker) = pending.pop() {
        budget.spend(
            seen.len().saturating_add(1),
            "Task automatic Drop actual origins",
        )?;
        if !seen.insert(worker) {
            continue;
        }
        let Some(target) = module.functions.get(worker.0 as usize) else {
            return Err(fail(
                span.unwrap_or_default(),
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
                return Err(fail(
                    span.unwrap_or_default(),
                    "pollable Task automatic Drop requires asynchronous continuation",
                ));
            }
        }
        budget.spend(target.blocks.len(), "Task Drop actual graph")?;
        for block in &target.blocks {
            for i in &block.instructions {
                budget.spend(1, "Task automatic Drop actual origins")?;
                match i.kind {
                    InstructionKind::AsyncRegistrationStart { .. } => {
                        return Err(fail(
                            span.unwrap_or_default(),
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
                    | InstructionKind::TaskStart {
                        worker: function, ..
                    } => pending.push(function),
                    InstructionKind::CallIndirect { .. } | InstructionKind::CallDynSlot { .. } => {
                        return Err(fail(
                            span.unwrap_or_default(),
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
