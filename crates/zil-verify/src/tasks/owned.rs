//! Independent closed inline-resource continuation profile. This checks actual
//! input storage AND every ordinary teardown's reachable code before allowing
//! synchronous close. A result-only Task type never proves either fact.
use super::*;
fn scalar(t: &Type) -> bool { matches!(t, Type::Unit | Type::Bool | Type::U8 | Type::U16 | Type::I32 | Type::I64) }
fn record(t: &Type, budget: &mut Budget) -> Check<bool> {
    resources::charge_payload(t, budget)?;
    let mut pending = vec![t];
    while let Some(t) = pending.pop() {
        budget.spend(1)?;
        match t {
            t if scalar(t) => {},
            Type::Record { fields, .. } => { budget.spend(fields.len())?; pending.extend(fields.iter().map(|(_, t)| t)); },
            _ => return Ok(false),
        }
    }
    Ok(true)
}
fn storage(t: &Type, budget: &mut Budget) -> Check<bool> {
    match t { Type::Managed(p) | Type::Desc(p) => record(p, budget), Type::Record { .. } => record(t, budget), t => Ok(scalar(t)) }
}
fn cleanup(module: &TypedModule, first: FunctionId, budget: &mut Budget, span: Option<Span>) -> Check<()> {
    let mut pending = vec![first]; let mut seen = BTreeSet::new();
    while let Some(id) = pending.pop() {
        budget.spend(seen.len().saturating_add(1))?;
        if !seen.insert(id) { continue; }
        let f = module.functions.get(id.0 as usize).ok_or_else(|| reject("E-VERIFY-TASK", span, "task cleanup target absent"))?;
        budget.spend(f.locals.len().saturating_add(f.values.len()).saturating_add(f.parameters.len()))?;
        if f.call_kind != CallKind::Direct || f.environment.is_some() || f.callable_contract.is_some() || f.callable_return_contract.is_some() || !scalar(&f.return_type) {
            return Err(reject("E-VERIFY-TASK", span, "task cleanup must have an actual closed synchronous Direct graph"));
        }
        if f.foreign.is_some() {
            if f.parameters.iter().any(|p| !scalar(&f.locals[p.0 as usize].ty)) || f.foreign.as_ref().is_some_and(|x| x.scoped_callback.is_some()) {
                return Err(reject("E-VERIFY-TASK", span, "task cleanup cannot publish frame storage to a foreign entry"));
            }
            continue;
        }
        for t in f.locals.iter().map(|l| &l.ty).chain(&f.values) {
            if !storage(t, budget)? { return Err(reject("E-VERIFY-TASK", span, "task cleanup retains unsupported owned or borrowed storage")); }
        }
        for t in f.locals.iter().map(|l| &l.ty).chain(&f.values) {
            if let Type::Managed(payload) = t {
                for _ in &module.resources { resources::charge_payload(payload, budget)?; }
                if let Some(drop) = module.resources.iter().find(|r| r.payload == **payload).and_then(|r| r.teardown) { pending.push(drop); }
            }
        }
        for b in &f.blocks { for i in &b.instructions {
            budget.spend(1)?;
            match &i.kind {
                InstructionKind::Call { function, .. } | InstructionKind::ForeignCall { function, .. } => pending.push(*function),
                InstructionKind::Constant(_) | InstructionKind::Load(_) | InstructionKind::Store { .. }
                | InstructionKind::StorageLive(_) | InstructionKind::StorageDead(_)
                | InstructionKind::ScopeEnter { .. } | InstructionKind::ScopeExit { .. }
                | InstructionKind::ReadDesc { .. } | InstructionKind::WriteDesc { .. }
                | InstructionKind::UnsafeEnter { .. } | InstructionKind::UnsafeExit { .. }
                | InstructionKind::MakeManaged { .. } | InstructionKind::DropManaged { .. }
                | InstructionKind::TakeLocal { .. } | InstructionKind::DescribeLocal { .. }
                | InstructionKind::Unary { .. } | InstructionKind::Binary { .. }
                | InstructionKind::ConvertInteger { .. } => {},
                _ => return Err(reject("E-VERIFY-TASK", Some(i.span), "task cleanup contains an asynchronous or external-user operation")),
            }
        }}
    }
    Ok(())
}
pub(super) fn profile(module: &TypedModule, f: &TypedFunction, budget: &mut Budget, span: Option<Span>) -> Check<Vec<FunctionId>> {
    if f.foreign.is_some() || f.environment.is_some() || f.callable_contract.is_some() || f.callable_return_contract.is_some() || f.reference_return_bound.is_some() || f.result_mode != PassingMode::Infer || f.return_type != Type::I32 || module.resources.iter().any(|r| r.teardown == Some(f.id)) {
        return Err(reject("E-VERIFY-TASK", span, "owned task worker requires a closed actual frame and scalar result"));
    }
    for parameter in &f.parameters {
        if !matches!(local_type(f, *parameter, span)?, Type::Managed(_)) && !scalar(local_type(f, *parameter, span)?) {
            return Err(reject("E-VERIFY-TASK", span, "task initial frame cannot retain borrowed input storage"));
        }
    }
    let mut checked = BTreeSet::new();
    for t in f.locals.iter().map(|l| &l.ty).chain(&f.values) {
        if !storage(t, budget)? { return Err(reject("E-VERIFY-TASK", span, "task owner needs a complete canonical inline resource frame slot")); }
        if let Type::Managed(payload) = t {
            for _ in &module.resources { resources::charge_payload(payload, budget)?; }
            let resource = module.resources.iter().find(|r| r.payload == **payload).ok_or_else(|| reject("E-VERIFY-TASK", span, "owned task slot has no actual resource"))?;
            if let Some(drop) = resource.teardown && checked.insert(drop) { cleanup(module, drop, budget, span)?; }
        }
    }
    let mut children = vec![];
    for b in &f.blocks { for i in &b.instructions {
        budget.spend(1)?;
        match &i.kind {
            InstructionKind::Call { function, .. } => cleanup(module, *function, budget, Some(i.span))?,
            InstructionKind::SuspendCall { function, arguments } => {
                budget.spend(arguments.len())?;
                if arguments.len() != 1 || value_type(f, arguments[0], Some(i.span))? != &Type::I32 { return Err(reject("E-VERIFY-TASK", Some(i.span), "owned parent currently requires scalar child inputs")); }
                children.push(*function);
            },
            InstructionKind::Constant(_) | InstructionKind::Load(_) | InstructionKind::Store { .. }
            | InstructionKind::StorageLive(_) | InstructionKind::StorageDead(_)
            | InstructionKind::ScopeEnter { .. } | InstructionKind::ScopeExit { .. }
            | InstructionKind::Unary { .. } | InstructionKind::Binary { .. } | InstructionKind::ConvertInteger { .. }
            | InstructionKind::TaskYield | InstructionKind::MakeManaged { .. } | InstructionKind::TakeLocal { .. }
            | InstructionKind::DescribeLocal { .. } | InstructionKind::ReadDesc { .. }
            | InstructionKind::WriteDesc { .. } | InstructionKind::DropManaged { .. } => {},
            _ => return Err(reject("E-VERIFY-TASK", Some(i.span), "task owned frame operation requires complete saved responsibility lowering")),
        }
    }}
    Ok(children)
}
