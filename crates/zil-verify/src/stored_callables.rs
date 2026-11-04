//! First qualified callback lane. Capture-free metadata is checked from the
//! actual definition; role selection still uses actual parameter positions.
use super::*;
fn bad(message: &str) -> Diagnostic {
    reject("E-VERIFY-QUALIFIED-CALLABLE", None, message)
}

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
    module.closures.get(id.0 as usize).is_some_and(|d| {
        d.kind == zil_ir::CallableKind::Fn
            && d.captures.is_empty()
            && d.lending_captures.is_empty()
            && d.storage_life.is_none()
            && scalar(&d.result)
    })
}
pub(super) fn value(module: &TypedModule, t: &Type) -> bool {
    let t = if let Type::Address { inner, .. } = t {
        inner.as_ref()
    } else {
        t
    };
    matches!(t,Type::Closure(id)if finite(module,*id))
}
pub(super) fn definitions(module: &TypedModule, b: &mut Budget) -> Check<()> {
    b.spend(module.closures.len())?;
    for d in &module.closures {
        if !finite(module, d.id) {
            return Err(unsupported(
                "qualified callbacks require actual capture-free Fn scalar entries",
                None,
            ));
        }
        super::borrowed_fp::signature_work(&d.parameters, &d.result, b)?;
        let f = module
            .functions
            .get(d.function.0 as usize)
            .ok_or_else(|| bad("qualified callback entry is absent"))?;
        b.spend(f.parameters.len())?;
        if f.environment != Some(d.id)
            || f.foreign.is_some()
            || f.call_kind != CallKind::Direct
            || f.parameters.len() != d.parameters.len()
            || f.return_type != d.result
        {
            return Err(bad("qualified callback complete actual entry differs"));
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
    let (ty, args) = match kind {
        InstructionKind::CallClosureBorrowed {
            callee,
            kind: zil_ir::CallableKind::Fn,
            arguments,
        } => (f.values.get(callee.0 as usize)?, arguments.as_slice()),
        InstructionKind::CallClosureShared { callee, arguments } => {
            (&f.locals.get(callee.0 as usize)?.ty, arguments.as_slice())
        }
        _ => return None,
    };
    let ty = if let Type::Address { inner, .. } = ty {
        inner.as_ref()
    } else {
        ty
    };
    let Type::Closure(id) = ty else { return None };
    finite(module, *id).then(|| (&module.closures[id.0 as usize], args))
}
pub(super) fn instruction(module: &TypedModule, f: &TypedFunction, i: &Instruction) -> bool {
    match &i.kind {
        InstructionKind::MakeClosure { closure, captures } => {
            finite(module, *closure) && captures.is_empty()
        }
        _ => called(module, f, &i.kind).is_some(),
    }
}
pub(super) fn types(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    b: &mut Budget,
) -> Check<()> {
    if let InstructionKind::MakeClosure { closure, captures } = &i.kind {
        if !captures.is_empty()
            || i.result.and_then(|v| f.values.get(v.0 as usize)) != Some(&Type::Closure(*closure))
        {
            return Err(bad(
                "qualified closure construction differs from actual definition",
            ));
        }
        return Ok(());
    }
    let (d, args) = called(module, f, &i.kind)
        .ok_or_else(|| bad("qualified callback call mode/entry is unsupported"))?;
    b.spend(args.len())?;
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
) -> Check<Option<&'a StorageLifeContract>> {
    let Some(contract) = &f.callable_contract else {
        return Ok(None);
    };
    let local = match kind {
        InstructionKind::CallClosureShared { callee, .. } => *callee,
        InstructionKind::CallClosureBorrowed { callee, .. } => {
            let mut current = *callee;
            let mut answer = None;
            b.spend(f.values.len())?;
            for _ in 0..f.values.len() {
                let mut found = None;
                b.spend(f.blocks.len())?;
                for block in &f.blocks {
                    b.spend(block.instructions.len())?;
                    for i in &block.instructions {
                        if i.result == Some(current) && found.replace(&i.kind).is_some() {
                            return Err(bad("callback receiver has duplicate definition"));
                        }
                    }
                }
                match found {
                    Some(InstructionKind::Load(local)) => {
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
    b.spend(f.parameters.len().saturating_add(contract.roles.len()))?;
    let position = f
        .parameters
        .iter()
        .position(|p| *p == local)
        .ok_or_else(|| bad("callback role receiver is not an actual declared parameter"))?;
    contract
        .roles
        .iter()
        .find(|r| r.parameter as usize == position)
        .map(|r| r.life.as_ref())
        .ok_or_else(|| bad("callback receiver has no exact parameter role"))
}

pub(super) fn roles(module: &TypedModule, graphs: &[FunctionGraph], b: &mut Budget) -> Check<()> {
    if module.closures.is_empty() {
        return Ok(());
    }
    b.spend(module.functions.len())?;
    for f in &module.functions {
        if let Some(contract) = &f.callable_contract {
            b.spend(contract.roles.len())?;
            for role in &contract.roles {
                if role.kind != zil_ir::CallableKind::Fn || !finite(module, role.closure) {
                    return Err(bad("qualified role requires actual capture-free Fn entry"));
                }
                let d = &module.closures[role.closure.0 as usize];
                b.spend(d.parameters.len())?;
                let needs=d.parameters.iter().any(|t|matches!(t,Type::BorrowedStorage{..})||matches!(t,Type::Address{inner,..}if matches!(inner.as_ref(),Type::BorrowedStorage{..})));
                if let Some(life) = &role.life {
                    let entry = &module.functions[d.function.0 as usize];
                    super::class_flow::check_role(
                        module,
                        entry,
                        &graphs[d.function.0 as usize].reachable,
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
