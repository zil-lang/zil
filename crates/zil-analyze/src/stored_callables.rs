//! Qualified callbacks with inline scalar environments. Metadata is checked
//! against the actual definition and receiver; roles use parameter positions.
use super::*;

pub(super) fn scalar(t: &Type) -> bool {
    matches!(
        t,
        Type::Unit
            | Type::Bool
            | Type::I32
            | Type::I64
            | Type::U64
            | Type::Usize
            | Type::U8
            | Type::U16
            | Type::F32
            | Type::F64
    )
}
pub(super) fn finite(module: &TypedModule, id: zil_ir::ClosureId) -> bool {
    module
        .closures
        .get(id.0 as usize)
        .is_some_and(|d| d.lending_captures.is_empty() && scalar(&d.result))
}
pub(super) fn fixed_value(module: &TypedModule, t: &Type) -> bool {
    let t = if let Type::Address { inner, .. } = t {
        inner.as_ref()
    } else {
        t
    };
    matches!(t, Type::Closure(id) if module.closures.get(id.0 as usize).is_some_and(|d| d.storage_life.is_some()))
}
pub(super) fn value(module: &TypedModule, t: &Type) -> bool {
    let t = if let Type::Address { inner, .. } = t {
        inner.as_ref()
    } else {
        t
    };
    matches!(t,Type::Closure(id)if finite(module,*id))
}
pub(super) fn capture(t: &Type) -> bool {
    scalar(t)
        || matches!(t, Type::Address { mutable: false, inner }
        if scalar(inner) || matches!(inner.as_ref(), Type::BorrowedStorage { storage, life }
            if matches!(storage.as_ref(), Type::Record { .. } | Type::Tuple(_)) && life.retained.is_empty()))
}
pub(super) fn definitions(module: &TypedModule, b: &mut Budget) -> CompileResult<()> {
    b.spend(module.closures.len(), "qualified callback inventory")?;
    for d in &module.closures {
        b.spend(d.captures.len(), "qualified scalar capture inventory")?;
        if !finite(module, d.id)
            || !d.captures.iter().all(|t| {
                if d.storage_life.is_some() {
                    capture(t)
                } else {
                    scalar(t)
                }
            })
        {
            return Err(vec![Diagnostic::unsupported(
                Phase::Analyze,
                "qualified callbacks require finite scalar/shared capture environments and scalar results",
                None,
            )]);
        }
        super::borrowed_fp::signature_work(&d.parameters, &d.result, b)?;
        let f = module
            .functions
            .get(d.function.0 as usize)
            .ok_or_else(|| bad("qualified callback entry is absent"))?;
        b.spend(
            f.parameters.len(),
            "qualified callback actual parameter correspondence",
        )?;
        if f.environment != Some(d.id)
            || f.foreign.is_some()
            || f.call_kind != CallKind::Direct
            || f.parameters.len() != d.parameters.len()
            || f.return_type != d.result
        {
            return Err(bad("qualified callback complete actual entry differs"));
        }
        if let Some(life) = &d.storage_life {
            b.spend(d.parameters.len(), "captured Fixed shared argument profile")?;
            if d.parameters
                .iter()
                .any(|t| matches!(t, Type::Address { mutable: true, .. }))
            {
                return Err(vec![Diagnostic::unsupported(
                    Phase::Analyze,
                    "captured Fixed exclusive arguments need capture target summary substitution",
                    None,
                )]);
            }
            super::signature_life::charge(life, b)?;
            if f.storage_life.as_ref() != Some(life) {
                return Err(bad(
                    "actual captured closure entry relation differs from definition",
                ));
            }
            super::signature_life::capture_relation(&d.parameters, &d.captures, &d.result, life)?;
        } else if f.storage_life.is_some() {
            return Err(bad("closure definition omits actual entry relation"));
        }
        for (p, t) in f.parameters.iter().zip(&d.parameters) {
            if f.locals.get(p.0 as usize).map(|p| &p.ty) != Some(t) {
                return Err(bad("qualified callback parameter differs from entry"));
            }
        }
    }
    Ok(())
}
pub(super) fn called<'a>(
    module: &'a TypedModule,
    f: &TypedFunction,
    kind: &'a InstructionKind,
) -> Option<(&'a ClosureDefinition, &'a [ValueId])> {
    let (id, args) = match kind {
        InstructionKind::CallClosureBorrowed {
            callee,
            kind,
            arguments,
        } => {
            let Type::Address { mutable, inner } = f.values.get(callee.0 as usize)? else {
                return None;
            };
            let Type::Closure(id) = inner.as_ref() else {
                return None;
            };
            let d = module.closures.get(id.0 as usize)?;
            if !matches!(
                (d.kind, kind),
                (zil_ir::CallableKind::Fn, zil_ir::CallableKind::Fn)
                    | (
                        zil_ir::CallableKind::Fn | zil_ir::CallableKind::FnMut,
                        zil_ir::CallableKind::FnMut
                    )
            ) || (*kind == zil_ir::CallableKind::FnMut && !mutable)
            {
                return None;
            }
            (*id, arguments.as_slice())
        }
        InstructionKind::CallClosureMut { callee, arguments } => {
            let local = f.locals.get(callee.0 as usize)?;
            let Type::Closure(id) = &local.ty else {
                return None;
            };
            let d = module.closures.get(id.0 as usize)?;
            if !local.mutable
                || !matches!(
                    d.kind,
                    zil_ir::CallableKind::Fn | zil_ir::CallableKind::FnMut
                )
            {
                return None;
            }
            (*id, arguments.as_slice())
        }
        InstructionKind::CallClosureOnce { callee, arguments } => {
            let Type::Closure(id) = f.values.get(callee.0 as usize)? else {
                return None;
            };
            (*id, arguments.as_slice())
        }
        InstructionKind::CallClosureShared { callee, arguments } => {
            let Type::Closure(id) = &f.locals.get(callee.0 as usize)?.ty else {
                return None;
            };
            if module.closures.get(id.0 as usize)?.kind != zil_ir::CallableKind::Fn {
                return None;
            }
            (*id, arguments.as_slice())
        }
        InstructionKind::CallClosure { callee, arguments } => {
            let Type::Closure(id) = f.values.get(callee.0 as usize)? else {
                return None;
            };
            if module.closures.get(id.0 as usize)?.kind != zil_ir::CallableKind::Fn {
                return None;
            }
            (*id, arguments.as_slice())
        }
        _ => return None,
    };
    finite(module, id).then(|| (&module.closures[id.0 as usize], args))
}

pub(super) fn instruction(module: &TypedModule, f: &TypedFunction, i: &Instruction) -> bool {
    match &i.kind {
        InstructionKind::MakeClosure { closure, captures } => {
            finite(module, *closure)
                && module.closures[closure.0 as usize].captures.len() == captures.len()
        }
        InstructionKind::ReadCapture { .. } | InstructionKind::WriteCapture { .. } => {
            f.environment.is_some_and(|id| finite(module, id))
        }
        InstructionKind::TakeLocal { local } | InstructionKind::DropClosure { local } => f
            .locals
            .get(local.0 as usize)
            .is_some_and(|l| value(module, &l.ty)),
        _ => called(module, f, &i.kind).is_some(),
    }
}
pub(super) fn types(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    b: &mut Budget,
) -> CompileResult<()> {
    if let InstructionKind::CallClosureOnce { callee, .. } = &i.kind
        && let Some(Type::Closure(id)) = f.values.get(callee.0 as usize)
        && let Some(d) = module.closures.get(id.0 as usize)
        && d.storage_life.is_some()
        && d.kind != CallableKind::FnOnce
    {
        return Err(vec![Diagnostic::unsupported(
            Phase::Analyze,
            "captured Fixed Once adaptation needs its actual environment consumption ledger",
            Some(i.span),
        )]);
    }
    if let InstructionKind::MakeClosure { closure, captures } = &i.kind {
        let d = module
            .closures
            .get(closure.0 as usize)
            .ok_or_else(|| bad("qualified environment definition is absent"))?;
        b.spend(captures.len(), "qualified environment operands")?;
        if d.storage_life.is_some() {
            for ty in &d.captures {
                b.type_work(ty, true, "actual captured environment type comparison")?;
            }
        }
        if captures.len() != d.captures.len()
            || d.captures
                .iter()
                .zip(captures)
                .any(|(t, v)| f.values.get(v.0 as usize) != Some(t))
            || i.result.and_then(|v| f.values.get(v.0 as usize)) != Some(&Type::Closure(*closure))
        {
            return Err(bad(
                "qualified closure construction differs from actual definition",
            ));
        }
        return Ok(());
    }
    match &i.kind {
        InstructionKind::ReadCapture { index } | InstructionKind::WriteCapture { index, .. } => {
            let d = f
                .environment
                .and_then(|id| module.closures.get(id.0 as usize))
                .ok_or_else(|| bad("qualified capture access lacks actual environment"))?;
            let t = d
                .captures
                .get(*index as usize)
                .ok_or_else(|| bad("qualified capture access lacks actual field"))?;
            if d.storage_life.is_some() {
                b.type_work(t, true, "actual captured field type comparison")?;
            }
            if let InstructionKind::WriteCapture { value, .. } = &i.kind {
                if d.kind == zil_ir::CallableKind::Fn
                    || !scalar(t)
                    || i.result.is_some()
                    || f.values.get(value.0 as usize) != Some(t)
                {
                    return Err(bad(
                        "qualified capture write requires exclusive environment and exact field",
                    ));
                }
            } else if i.result.and_then(|v| f.values.get(v.0 as usize)) != Some(t) {
                return Err(bad(
                    "qualified capture read result differs from actual field",
                ));
            }
            return Ok(());
        }
        InstructionKind::TakeLocal { local } => {
            if i.result.and_then(|v| f.values.get(v.0 as usize))
                != f.locals.get(local.0 as usize).map(|l| &l.ty)
            {
                return Err(bad("qualified environment move changes actual type"));
            }
            return Ok(());
        }
        InstructionKind::DropClosure { .. } => {
            if i.result.is_some() {
                return Err(bad("qualified environment cleanup fabricates result"));
            }
            return Ok(());
        }
        _ => {}
    }
    let (d, args) = called(module, f, &i.kind)
        .ok_or_else(|| bad("qualified callback call mode/entry is unsupported"))?;
    b.spend(args.len(), "qualified callback actual arguments")?;
    if args.len() != d.parameters.len() {
        return Err(bad("qualified callback argument arity differs"));
    }
    super::borrowed_fp::signature_work(&d.parameters, &d.result, b)?;
    for (t, v) in d.parameters.iter().zip(args) {
        if f.values.get(v.0 as usize) != Some(t) {
            return Err(bad("qualified callback argument type differs"));
        }
    }
    if d.result == Type::Unit {
        if i.result.is_some() {
            return Err(bad("Unit callback fabricates result"));
        }
    } else if i.result.and_then(|v| f.values.get(v.0 as usize)) != Some(&d.result) {
        return Err(bad("qualified callback result differs"));
    }
    Ok(())
}
/// An ambiguous/rebound receiver cannot select a role merely by ClosureId.
pub(super) fn role<'a>(
    f: &'a TypedFunction,
    kind: &InstructionKind,
    b: &mut Budget,
) -> CompileResult<Option<&'a StorageLifeContract>> {
    let Some(contract) = &f.callable_contract else {
        return Ok(None);
    };
    let local = match kind {
        InstructionKind::CallClosureShared { callee, .. }
        | InstructionKind::CallClosureMut { callee, .. } => *callee,
        InstructionKind::CallClosureBorrowed { callee, .. }
        | InstructionKind::CallClosure { callee, .. }
        | InstructionKind::CallClosureOnce { callee, .. } => {
            let mut current = *callee;
            let mut answer = None;
            b.spend(f.values.len(), "callback receiver chain bound")?;
            for _ in 0..f.values.len() {
                let mut found = None;
                b.spend(f.blocks.len(), "callback receiver block inventory")?;
                for block in &f.blocks {
                    b.spend(
                        block.instructions.len(),
                        "actual callback receiver definition scan",
                    )?;
                    for i in &block.instructions {
                        if i.result == Some(current) && found.replace(&i.kind).is_some() {
                            return Err(bad("callback receiver has duplicate definition"));
                        }
                    }
                }
                match found {
                    Some(InstructionKind::Load(local))
                    | Some(InstructionKind::TakeLocal { local }) => {
                        answer = Some(*local);
                        break;
                    }
                    Some(InstructionKind::ReborrowAddress { address, .. }) => current = *address,
                    _ => break,
                }
            }
            answer
                .ok_or_else(|| bad("callback receiver role has no exact actual parameter origin"))?
        }
        _ => return Ok(None),
    };
    b.spend(
        f.parameters.len().saturating_add(contract.roles.len()),
        "callback role actual parameter position",
    )?;
    let position = f
        .parameters
        .iter()
        .position(|p| *p == local)
        .ok_or_else(|| bad("callback role receiver is not an actual declared parameter"))?;
    let role = contract
        .roles
        .iter()
        .find(|r| r.parameter as usize == position)
        .ok_or_else(|| bad("callback receiver has no exact parameter role"))?;
    let mode = match kind {
        InstructionKind::CallClosureBorrowed { kind, .. } => *kind,
        InstructionKind::CallClosureMut { .. } => zil_ir::CallableKind::FnMut,
        InstructionKind::CallClosureOnce { .. } => zil_ir::CallableKind::FnOnce,
        _ => zil_ir::CallableKind::Fn,
    };
    if mode != role.kind {
        return Err(bad(
            "qualified call mode differs from its actual declared role",
        ));
    }
    Ok(role.life.as_ref())
}

pub(super) fn roles(
    module: &TypedModule,
    graphs: &[(Vec<Vec<usize>>, Vec<bool>)],
    b: &mut Budget,
) -> CompileResult<()> {
    if module.closures.is_empty() {
        return Ok(());
    }
    b.spend(
        module.functions.len(),
        "qualified callback role entry inventory",
    )?;
    for f in &module.functions {
        if let Some(contract) = &f.callable_contract {
            b.spend(contract.roles.len(), "distinct qualified callback roles")?;
            for role in &contract.roles {
                if !finite(module, role.closure) {
                    return Err(bad(
                        "qualified role requires actual finite environment entry",
                    ));
                }
                let d = &module.closures[role.closure.0 as usize];
                b.spend(d.parameters.len(), "qualified role signature presence")?;
                let needs=d.parameters.iter().any(|t|matches!(t,Type::BorrowedStorage{..})||matches!(t,Type::Address{inner,..}if matches!(inner.as_ref(),Type::BorrowedStorage{..})));
                if let Some(life) = &role.life {
                    let entry = &module.functions[d.function.0 as usize];
                    super::class_flow::check_role(
                        module,
                        entry,
                        &graphs[d.function.0 as usize].1,
                        life,
                        b,
                    )?;
                } else if needs {
                    return Err(bad(
                        "qualified role omits its complete semantic class relation",
                    ));
                }
            }
        }
    }
    Ok(())
}
