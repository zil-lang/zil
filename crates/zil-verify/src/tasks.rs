//! Independent scalar task ABI admission. No producer call-mode flag is trusted.
use super::*;
pub(super) fn carrier(module: &TypedModule, ty: &Type, span: Option<Span>, budget: &mut Budget) -> Check<()> {
    budget.spend(module.resources.len())?;
    if !module.target.is_supported_x64_llvm() || !matches!(ty, Type::Managed(p) | Type::Desc(p) if **p == Type::TaskI32) || !module.resources.iter().any(|r| r.name == TASK_I32_RESOURCE_NAME && r.payload == Type::TaskI32 && r.teardown.is_none()) { return Err(reject("E-VERIFY-TASK", span, "task carrier requires canonical sealed native task definition")); }
    Ok(())
}
pub(super) fn instruction(module: &TypedModule, f: &TypedFunction, i: &Instruction, budget: &mut Budget) -> Check<Option<Type>> {
    budget.tasks = true;
    budget.spend(1)?;
    let span = Some(i.span);
    let bad = || reject("E-VERIFY-TASK", span, "task operand differs from exact sealed ABI");
    Ok(Some(match i.kind {
        InstructionKind::TaskStart { worker, argument } => {
            if value_type(f, argument, span)? != &Type::I32 { return Err(bad()); }
            profile(module, worker, budget, span)?;
            let ty = Type::Managed(Box::new(Type::TaskI32)); carrier(module, &ty, span, budget)?; ty
        }
        InstructionKind::TaskYield => { profile(module, f.id, budget, span)?; Type::Unit }
        InstructionKind::TaskValid { task } => {
            let ty = value_type(f, task, span)?;
            if !matches!(ty, Type::Desc(p) if **p == Type::TaskI32) { return Err(bad()); }
            carrier(module, ty, span, budget)?; Type::Bool
        }
        InstructionKind::TaskFinishDestroy { task } => {
            let ty = value_type(f, task, span)?;
            if !matches!(ty, Type::Managed(p) if **p == Type::TaskI32) { return Err(bad()); }
            carrier(module, ty, span, budget)?; Type::Unit
        }
        InstructionKind::TaskPoll { task } | InstructionKind::TaskRequestCancel { task } | InstructionKind::TaskPollClose { task } => {
            let ty = value_type(f, task, span)?;
            if !ty.is_exclusive_task_head_address() { return Err(bad()); }
            let Type::Address { inner, .. } = ty else { unreachable!() }; carrier(module, inner, span, budget)?;
            match i.kind { InstructionKind::TaskPoll { .. } => Type::Tuple(vec![Type::I32, Type::I32]), InstructionKind::TaskPollClose { .. } => Type::Bool, _ => Type::Unit }
        }
        _ => unreachable!(),
    }))
}
fn profile(module: &TypedModule, id: FunctionId, budget: &mut Budget, span: Option<Span>) -> Check<()> {
    let f = module.functions.get(id.0 as usize).ok_or_else(|| reject("E-VERIFY-TASK", span, "task worker is absent"))?;
    budget.spend(f.locals.len().saturating_add(f.values.len()).saturating_add(module.resources.len()).saturating_add(1))?;
    let scalar = |t: &Type| matches!(t, Type::Unit | Type::Bool | Type::U8 | Type::U16 | Type::I32 | Type::I64);
    if f.foreign.is_some() || f.environment.is_some() || f.callable_contract.is_some() || f.callable_return_contract.is_some() || f.reference_return_bound.is_some() || f.parameters.len() != 1 || local_type(f, f.parameters[0], span)? != &Type::I32 || f.result_mode != PassingMode::Infer || f.return_type != Type::I32 || f.locals.iter().any(|l| !scalar(&l.ty)) || f.values.iter().any(|ty| !scalar(ty)) || module.resources.iter().any(|r| r.teardown == Some(id)) { return Err(reject("E-VERIFY-TASK", span, "task worker must prove the closed scalar profile independently")); }
    for b in &f.blocks { budget.spend(1)?; for i in &b.instructions { budget.spend(1)?;
        match i.kind {
            InstructionKind::TaskYield | InstructionKind::StorageLive(_) | InstructionKind::StorageDead(_) | InstructionKind::Load(_) | InstructionKind::Store { .. } | InstructionKind::Unary { .. } | InstructionKind::Binary { .. } | InstructionKind::ConvertInteger { .. } | InstructionKind::Constant(Constant::Unit | Constant::Bool(_) | Constant::U8(_) | Constant::U16(_) | Constant::I32(_) | Constant::I64(_)) => {},
            _ => return Err(reject("E-VERIFY-TASK", Some(i.span), "task worker operation has no closed scalar suspension lowering")),
        }
    }}
    Ok(())
}
pub(super) fn module(module: &TypedModule, budget: &mut Budget) -> Check<()> {
    if !budget.tasks { return Ok(()); }
    budget.spend(module.functions.len())?;
    let mut modes = vec![false; module.functions.len()];
    let mut edges = Vec::new();
    for f in &module.functions { for b in &f.blocks { for i in &b.instructions { budget.spend(1)?;
        match i.kind {
            InstructionKind::TaskYield => modes[f.id.0 as usize] = true,
            InstructionKind::Call { function, .. } => { budget.spend(1)?; edges.push((f.id, function)); },
            _ => {},
        }
    }}}
    loop { let mut changed = false;
        for &(caller, callee) in &edges { budget.spend(1)?;
            let to = modes.get(callee.0 as usize).ok_or_else(|| reject("E-VERIFY-TASK", None, "call target is absent"))?;
            if *to && !modes[caller.0 as usize] { modes[caller.0 as usize] = true; changed = true; }
        }
        if !changed { break; }
    }
    for f in &module.functions { for b in &f.blocks { for i in &b.instructions { budget.spend(1)?;
        let target = match i.kind { InstructionKind::Call { function, .. } | InstructionKind::FunctionRef { function } => Some(function), InstructionKind::WithScopedThread { worker, .. } => Some(worker), _ => None };
        if target.is_some_and(|id| modes.get(id.0 as usize).copied().unwrap_or(false)) { return Err(reject("E-VERIFY-TASK", Some(i.span), "actual Suspend function has no ordinary Direct execution authority")); }
    }}}
    Ok(())
}
