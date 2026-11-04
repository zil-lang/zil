//! Producer reconstruction of closed inline-resource continuation profile. This checks actual
//! input storage AND every ordinary teardown's reachable code before allowing
//! synchronous close. A result-only Task type never proves either fact.
use super::*;
fn local_type(f: &TypedFunction, local: LocalId, span: Option<zil_base::Span>) -> CompileResult<&Type> { f.locals.get(local.0 as usize).map(|l| &l.ty).ok_or_else(|| fail(span.unwrap_or(f.span), "owned task local absent")) }
fn value_type(f: &TypedFunction, value: ValueId, span: Option<zil_base::Span>) -> CompileResult<&Type> { f.values.get(value.0 as usize).ok_or_else(|| fail(span.unwrap_or(f.span), "owned task value absent")) }
fn reject(_: &str, span: Option<zil_base::Span>, message: &str) -> Vec<Diagnostic> { fail(span.unwrap_or_default(), message) }
fn scalar(t: &Type) -> bool { matches!(t, Type::Unit | Type::Bool | Type::U8 | Type::U16 | Type::I32 | Type::I64) }
fn record(t: &Type, budget: &mut Budget) -> CompileResult<bool> {
    budget.type_work(t, false, "actual owned task type")?;
    let mut pending = vec![t];
    while let Some(t) = pending.pop() {
        budget.spend(1, "actual owning task profile")?;
        match t {
            t if scalar(t) => {},
            Type::Record { fields, .. } => { budget.spend(fields.len(), "actual owning task profile")?; pending.extend(fields.iter().map(|(_, t)| t)); },
            _ => return Ok(false),
        }
    }
    Ok(true)
}
pub(super) fn result_type(t: &Type, budget: &mut Budget) -> CompileResult<bool> {
    if *t == Type::I32 { return Ok(true); }
    if let Type::Managed(payload) = t { return record(payload, budget); }
    Ok(false)
}
fn package(t: &Type, budget: &mut Budget) -> CompileResult<bool> {
    let Type::Package { fields, .. } = t else { return Ok(false); };
    budget.spend(fields.len(), "actual single-owner package")?;
    let mut owners=0;
    for (_, field) in fields {
        if let Type::Managed(payload)=field { owners+=1; if !record(payload,budget)? {return Ok(false);} }
        else if !record(field,budget)? {return Ok(false);}
    }
    Ok(owners==1)
}
fn tuple(t: &Type, budget: &mut Budget) -> CompileResult<bool> {
    let Type::Tuple(fields)=t else {return Ok(false);};
    for field in fields {
        if let Type::Managed(payload)=field {if !record(payload,budget)? {return Ok(false);}}
        else if !record(field,budget)? {return Ok(false);}
    }
    Ok(true)
}
fn storage(t: &Type, budget: &mut Budget) -> CompileResult<bool> {
    match t { Type::Managed(p) | Type::Desc(p) => record(p, budget), Type::Record { .. } => record(t, budget), Type::Package { .. } => package(t, budget), Type::Tuple(_) => tuple(t,budget), Type::Address {inner,..} => Ok(scalar(inner)), t => Ok(scalar(t)) }
}
fn cleanup(module: &TypedModule, first: FunctionId, budget: &mut Budget, span: Option<zil_base::Span>) -> CompileResult<()> {
    let mut pending = vec![first]; let mut seen = BTreeSet::new();
    while let Some(id) = pending.pop() {
        budget.spend(seen.len().saturating_add(1), "actual owning task profile")?;
        if !seen.insert(id) { continue; }
        let f = module.functions.get(id.0 as usize).ok_or_else(|| reject("E-VERIFY-TASK", span, "task cleanup target absent"))?;
        budget.spend(f.locals.len().saturating_add(f.values.len()).saturating_add(f.parameters.len()), "actual owning task profile")?;
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
                for _ in &module.resources { budget.type_work(payload, false, "actual cleanup resource equality")?; }
                if let Some(drop) = module.resources.iter().find(|r| r.payload == **payload).and_then(|r| r.teardown) { pending.push(drop); }
            }
        }
        for b in &f.blocks { for i in &b.instructions {
            budget.spend(1, "actual owning task profile")?;
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
pub(super) fn profile(module: &TypedModule, f: &TypedFunction, budget: &mut Budget, span: Option<zil_base::Span>) -> CompileResult<Vec<FunctionId>> {
    if f.foreign.is_some() || f.environment.is_some() || f.callable_contract.is_some() || f.callable_return_contract.is_some() || f.reference_return_bound.is_some() || !(f.result_mode == PassingMode::Infer && f.return_type == Type::I32 || f.result_mode == PassingMode::Give && matches!(f.return_type, Type::Managed(_)) && result_type(&f.return_type, budget)?) || module.resources.iter().any(|r| r.teardown == Some(f.id)) {
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
        let leaves = match t { Type::Package { fields, .. } => fields.iter().map(|(_,t)|t).collect::<Vec<_>>(), Type::Tuple(fields)=>fields.iter().collect(), _=>vec![t] };
        for leaf in leaves { if let Type::Managed(payload) = leaf {
            for _ in &module.resources { budget.type_work(payload, false, "actual task resource equality")?; }
            let resource = module.resources.iter().find(|r| r.payload == **payload).ok_or_else(|| reject("E-VERIFY-TASK", span, "owned task slot has no actual resource"))?;
            if let Some(drop) = resource.teardown && checked.insert(drop) { cleanup(module, drop, budget, span)?; }
        }}
    }
    let mut children = vec![];
    for b in &f.blocks { for i in &b.instructions {
        budget.spend(1, "actual owning task profile")?;
        match &i.kind {
            InstructionKind::Call { function, .. } => cleanup(module, *function, budget, Some(i.span))?,
            InstructionKind::SuspendCall { function, arguments } => {
                budget.spend(arguments.len(), "actual owning task profile")?;
                if arguments.len() != 1 || value_type(f, arguments[0], Some(i.span))? != &Type::I32 { return Err(reject("E-VERIFY-TASK", Some(i.span), "owned parent currently requires scalar child inputs")); }
                children.push(*function);
            },
            InstructionKind::Constant(_) | InstructionKind::Load(_) | InstructionKind::Store { .. }
            | InstructionKind::StorageLive(_) | InstructionKind::StorageDead(_)
            | InstructionKind::ScopeEnter { .. } | InstructionKind::ScopeExit { .. }
            | InstructionKind::Unary { .. } | InstructionKind::Binary { .. } | InstructionKind::ConvertInteger { .. }
            | InstructionKind::TaskYield | InstructionKind::MakeManaged { .. } | InstructionKind::TakeLocal { .. }
            | InstructionKind::DescribeLocal { .. } | InstructionKind::ReadDesc { .. }
            | InstructionKind::WriteDesc { .. } | InstructionKind::DropManaged { .. }
            | InstructionKind::MakeOwnedTuple { .. } | InstructionKind::DropTuple { .. }
            | InstructionKind::BorrowFieldLocal { .. } | InstructionKind::BorrowPlaceLocal { .. } | InstructionKind::ReadAddress { .. }
            | InstructionKind::MakePackage { .. } | InstructionKind::DropPackage { .. }
            | InstructionKind::TakePackageField { .. } | InstructionKind::StorePackageField { .. } | InstructionKind::DropPackageField { .. } => {},
            _ => return Err(reject("E-VERIFY-TASK", Some(i.span), "task owned frame operation requires complete saved responsibility lowering")),
        }
    }}
    Ok(children)
}
