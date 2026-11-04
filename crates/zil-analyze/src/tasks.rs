//! Producer checks reconstruct the scalar task profile and actual suspension.
use super::*;
fn fail(span: zil_base::Span, message: &str) -> Vec<Diagnostic> {
    vec![Diagnostic::error(Phase::Analyze, "E-TASK-ABI", Some(span), message)]
}
fn scalar(ty: &Type) -> bool { matches!(ty, Type::Unit | Type::Bool | Type::U8 | Type::U16 | Type::I32 | Type::I64) }
fn worker(module: &TypedModule, id: FunctionId, budget: &mut Budget, span: zil_base::Span) -> CompileResult<()> {
    let f = module.functions.get(id.0 as usize).ok_or_else(|| fail(span, "task worker is absent"))?;
    budget.spend(f.locals.len().saturating_add(f.values.len()).saturating_add(module.resources.len()).saturating_add(1), "task worker real scalar profile")?;
    if f.foreign.is_some() || f.environment.is_some() || f.callable_contract.is_some() || f.callable_return_contract.is_some() || f.reference_return_bound.is_some() || f.result_mode != PassingMode::Infer || f.parameters.len() != 1 || f.locals.get(f.parameters[0].0 as usize).is_none_or(|l| l.ty != Type::I32) || f.return_type != Type::I32 || f.locals.iter().any(|l| !scalar(&l.ty)) || f.values.iter().any(|t| !scalar(t)) || module.resources.iter().any(|r| r.teardown == Some(id)) {
        return Err(fail(span, "task worker requires a closed scalar fn(i32)->i32 without external users or cleanup"));
    }
    for b in &f.blocks {
        budget.spend(1, "task worker block")?;
        for i in &b.instructions {
            budget.spend(1, "task worker instruction")?;
            if !matches!(i.kind, InstructionKind::Constant(Constant::Unit | Constant::Bool(_) | Constant::U8(_) | Constant::U16(_) | Constant::I32(_) | Constant::I64(_)) | InstructionKind::Load(_) | InstructionKind::Store { .. } | InstructionKind::StorageLive(_) | InstructionKind::StorageDead(_) | InstructionKind::Unary { .. } | InstructionKind::Binary { .. } | InstructionKind::ConvertInteger { .. } | InstructionKind::TaskYield) {
                return Err(fail(i.span, "scalar task worker contains an external, owning or unsupported operation"));
            }
        }
    }
    Ok(())
}
pub(super) fn check(module: &TypedModule, budget: &mut Budget) -> CompileResult<()> {
    if !budget.tasks { return Ok(()); }
    budget.spend(module.functions.len(), "task suspension inventory allocation")?;
    let mut suspend = vec![false; module.functions.len()];
    for (index, f) in module.functions.iter().enumerate() {
        for block in &f.blocks { for i in &block.instructions {
            budget.spend(1, "task suspension inventory")?;
            let ty = |v: ValueId| f.values.get(v.0 as usize).ok_or_else(|| fail(i.span, "task operand is absent"));
            let expected = match i.kind {
                InstructionKind::TaskStart { worker: id, argument } => { if ty(argument)? != &Type::I32 { return Err(fail(i.span, "task payload must be i32")); } worker(module, id, budget, i.span)?; Some(Type::Managed(Box::new(Type::TaskI32))) }
                InstructionKind::TaskYield => { suspend[index] = true; worker(module, f.id, budget, i.span)?; Some(Type::Unit) }
                InstructionKind::TaskValid { task } => { if !matches!(ty(task)?, Type::Desc(p) if **p == Type::TaskI32) { return Err(fail(i.span, "task validity requires its actual descriptor")); } Some(Type::Bool) }
                InstructionKind::TaskPoll { task } | InstructionKind::TaskRequestCancel { task } | InstructionKind::TaskPollClose { task } => {
                    if !ty(task)?.is_exclusive_task_head_address() { return Err(fail(i.span, "task activation requires exclusive actual owning head")); }
                    Some(match i.kind { InstructionKind::TaskPoll { .. } => Type::Tuple(vec![Type::I32, Type::I32]), InstructionKind::TaskPollClose { .. } => Type::Bool, _ => Type::Unit })
                }
                InstructionKind::TaskFinishDestroy { task } => { if !matches!(ty(task)?, Type::Managed(p) if **p == Type::TaskI32) { return Err(fail(i.span, "task destruction consumes its actual owner")); } Some(Type::Unit) }
                _ => None,
            };
            if let Some(expected) = expected {
                if i.result.and_then(|v| f.values.get(v.0 as usize)) != Some(&expected) { return Err(fail(i.span, "task instruction result differs from sealed ABI")); }
            }
        }}
    }
    loop {
        let mut changed = false;
        for f in &module.functions { for block in &f.blocks { for i in &block.instructions {
            budget.spend(1, "derive actual Suspend call closure")?;
            if let InstructionKind::Call { function, .. } = i.kind {
                let callee = suspend.get(function.0 as usize).ok_or_else(|| fail(i.span, "call target is absent"))?;
                if *callee && !suspend[f.id.0 as usize] { suspend[f.id.0 as usize] = true; changed = true; }
            }
        }}}
        if !changed { break; }
    }
    for f in &module.functions { for b in &f.blocks { for i in &b.instructions {
        budget.spend(1, "Suspend Direct boundary exclusion")?;
        let direct = match i.kind { InstructionKind::Call { function, .. } | InstructionKind::FunctionRef(function) => Some(function), InstructionKind::WithScopedThread { worker, .. } => Some(worker), _ => None };
        if direct.is_some_and(|id| suspend.get(id.0 as usize).copied().unwrap_or(false)) { return Err(fail(i.span, "Suspend function cannot be called, referenced or started through a Direct ABI")); }
    }}}
    Ok(())
}
