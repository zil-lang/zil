//! Actual head origins and reachable worker graph, never a producer flag.
use super::*;
#[derive(Clone, Default, PartialEq, Eq)]
struct Origins {
    unknown: bool,
    workers: BTreeSet<FunctionId>,
}
impl Origins {
    fn add(&mut self, other: &Self, budget: &mut Budget) -> CompileResult<bool> {
        budget.spend(
            self.workers
                .len()
                .saturating_add(other.workers.len())
                .saturating_add(1),
            "Task automatic Drop actual origins",
        )?;
        let old = self.clone();
        self.unknown |= other.unknown;
        self.workers.extend(other.workers.iter().copied());
        Ok(*self != old)
    }
}
fn contains_task(module: &TypedModule, t: &Type, budget: &mut Budget) -> CompileResult<bool> {
    budget.type_work(t, false, "Task automatic Drop actual type")?;
    let mut pending = vec![t];
    let mut closures = BTreeSet::new();
    while let Some(t) = pending.pop() {
        budget.spend(1, "Task automatic Drop actual origins")?;
        match t {
            Type::TaskI32 | Type::Task { .. } => return Ok(true),
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
            Type::Array { element, .. } => pending.push(element),
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
pub(crate) fn require(
    module: &TypedModule,
    f: &TypedFunction,
    local: LocalId,
    budget: &mut Budget,
    span: Option<zil_base::Span>,
) -> CompileResult<()> {
    let ty = local_type(f, local, span)?;
    if !contains_task(module, ty, budget)? {
        return Ok(());
    }
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
        relevant.push(contains_task(module, t, budget)?);
    }
    for p in &f.parameters {
        if contains_task(module, &f.locals[p.0 as usize].ty, budget)? {
            locals[p.0 as usize].unknown = true;
        }
    }
    loop {
        let mut changed = false;
        budget.spend(f.blocks.len(),"Task Drop actual graph")?;
        for block in &f.blocks {
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
                        budget.spend(locals[local.0 as usize].workers.len().saturating_add(1),"Task Drop actual graph")?;
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
                    | InstructionKind::TakeLocal { local }
                    | InstructionKind::TakePackageField { local, .. }
                    | InstructionKind::TakeOwnedArrayElement { local, .. }
                    | InstructionKind::ReadVariantLocal { local, .. } => {
                        budget.spend(
                            locals[local.0 as usize].workers.len().saturating_add(1),
                            "Task automatic Drop actual origins",
                        )?;
                        locals[local.0 as usize].clone()
                    }
                    InstructionKind::ReadErrorUnion { value, .. }
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
    budget.spend(
        locals[local.0 as usize].workers.len().saturating_add(1),
        "Task automatic Drop actual origins",
    )?;
    let origins = &locals[local.0 as usize];
    if origins.unknown || origins.workers.is_empty() {
        return Err(fail(
            span.unwrap_or(f.span),
            "Task automatic Drop has no complete synchronous worker origin",
        ));
    }
    let mut pending = origins.workers.iter().copied().collect::<Vec<_>>();
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
                span.unwrap_or(f.span),
                "Task automatic Drop worker absent",
            ));
        };
        budget.spend(target.blocks.len(),"Task Drop actual graph")?;
        for block in &target.blocks {
            for i in &block.instructions {
                budget.spend(1, "Task automatic Drop actual origins")?;
                match i.kind {
                    InstructionKind::AsyncRegistrationStart { .. } => {
                        return Err(fail(
                            span.unwrap_or(f.span),
                            "pollable Task automatic Drop requires asynchronous continuation",
                        ));
                    }
                    InstructionKind::SuspendCall { function, .. } => pending.push(function),
                    _ => {}
                }
            }
        }
    }
    Ok(())
}
