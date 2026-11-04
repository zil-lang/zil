//! Actual head origins and reachable worker graph, never a producer flag.
use super::*;
#[derive(Clone, Default, PartialEq, Eq)]
struct Origins {
    unknown: bool,
    workers: BTreeSet<FunctionId>,
}
impl Origins {
    fn add(&mut self, other: &Self, budget: &mut Budget) -> Check<bool> {
        budget.spend(
            self.workers
                .len()
                .saturating_add(other.workers.len())
                .saturating_add(1),
        )?;
        let old = self.clone();
        self.unknown |= other.unknown;
        self.workers.extend(other.workers.iter().copied());
        Ok(*self != old)
    }
}
fn contains_kind(
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
pub(crate) fn require(
    module: &TypedModule,
    f: &TypedFunction,
    local: LocalId,
    budget: &mut Budget,
    span: Option<zil_base::Span>,
) -> Check<()> {
    require_origin(module, f, Some(local), None, budget, span)
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
    for p in &f.parameters {
        if contains_kind(module, &f.locals[p.0 as usize].ty, budget, false)? {
            locals[p.0 as usize].unknown = true;
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
                if let InstructionKind::UnpackTuple { local, fields } = &i.kind {
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
                    InstructionKind::TaskCreate { worker, .. }
                    | InstructionKind::TaskStart { worker, .. } => Origins {
                        unknown: false,
                        workers: BTreeSet::from([*worker]),
                    },
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
                    },
                };
                changed |= values[out.0 as usize].add(&origin, budget)?;
            }
        }
        if !changed {
            break;
        }
    }
    let origins = if let Some(local) = local {
        &locals[local.0 as usize]
    } else {
        &values[value.expect("value endpoint").0 as usize]
    };
    budget.spend(origins.workers.len().saturating_add(1))?;
    if origins.unknown || origins.workers.is_empty() {
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
