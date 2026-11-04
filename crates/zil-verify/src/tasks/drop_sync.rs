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
fn contains_task(t: &Type, budget: &mut Budget) -> Check<bool> {
    resources::charge_payload(t, budget)?;
    let mut pending = vec![t];
    while let Some(t) = pending.pop() {
        budget.spend(1)?;
        match t {
            Type::TaskI32 | Type::Task { .. } => return Ok(true),
            Type::Managed(p) | Type::Desc(p) | Type::Address { inner: p, .. } => pending.push(p),
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
) -> Check<()> {
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
        relevant.push(contains_task(t, budget)?);
    }
    for p in &f.parameters {
        if contains_task(&f.locals[p.0 as usize].ty, budget)? {
            locals[p.0 as usize].unknown = true;
        }
    }
    loop {
        let mut changed = false;
        for block in &f.blocks {
            for i in &block.instructions {
                budget.spend(1)?;
                if let InstructionKind::Store { local, value } = i.kind {
                    changed |= locals[local.0 as usize].add(&values[value.0 as usize], budget)?;
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
                    InstructionKind::Load(local) | InstructionKind::TakeLocal { local } => {
                        budget.spend(locals[local.0 as usize].workers.len().saturating_add(1))?;
                        locals[local.0 as usize].clone()
                    }
                    InstructionKind::ReadErrorUnion { value, .. }
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
                    | InstructionKind::MakeVariant { fields, .. } => {
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
    budget.spend(locals[local.0 as usize].workers.len().saturating_add(1))?;
    let origins = &locals[local.0 as usize];
    if origins.unknown || origins.workers.is_empty() {
        return Err(reject(
            "E-VERIFY-TASK",
            span,
            "Task automatic Drop has no complete synchronous worker origin",
        ));
    }
    let mut pending = origins.workers.iter().copied().collect::<Vec<_>>();
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
                    InstructionKind::SuspendCall { function, .. } => pending.push(function),
                    _ => {}
                }
            }
        }
    }
    Ok(())
}
