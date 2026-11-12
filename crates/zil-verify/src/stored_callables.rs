//! Qualified callbacks with inline scalar environments. Metadata is checked
//! against the actual definition and receiver; roles use parameter positions.
use super::*;
#[path = "stored_callables_staged.rs"]
mod staged;
#[path = "stored_callables_staged_cfg.rs"]
mod staged_cfg;
#[path = "stored_callables_staged_tail.rs"]
mod staged_tail;
fn bad(message: &str) -> Diagnostic {
    reject("E-VERIFY-QUALIFIED-CALLABLE", None, message)
}

pub(super) fn scalar(t: &Type) -> bool {
    matches!(
        t,
        Type::Unit
            | Type::Bool
            | Type::I32
            | Type::I8
            | Type::I16
            | Type::Isize
            | Type::I64
            | Type::U64
            | Type::Usize
            | Type::U8
            | Type::U16
            | Type::U32
            | Type::F32
            | Type::F64
    )
}
pub(super) fn scalar_foreign(f: &TypedFunction, b: &mut Budget) -> Check<bool> {
    if f.foreign.is_none() {
        return Ok(false);
    }
    b.spend(f.parameters.len().saturating_add(1))?;
    if !f.blocks.is_empty()
        || !f.values.is_empty()
        || f.environment.is_some()
        || f.call_kind != CallKind::Direct
        || f.result_mode != PassingMode::Infer
        || f.storage_life.is_some()
        || f.reference_return_bound.is_some()
        || f.callable_contract.is_some()
        || f.callable_return_contract.is_some()
        || !scalar(&f.return_type)
    {
        return Ok(false);
    }
    for parameter in &f.parameters {
        if !f
            .locals
            .get(parameter.0 as usize)
            .is_some_and(|l| scalar(&l.ty))
        {
            return Ok(false);
        }
    }
    Ok(true)
}

pub(super) fn lending(d: &ClosureDefinition) -> bool {
    d.storage_life.is_some()
        && !d.lending_captures.is_empty()
        && d.kind != CallableKind::FnOnce
        && matches!(&d.result, Type::Address { mutable:false, inner } if scalar(inner))
}
pub(super) fn life_result(d: &ClosureDefinition) -> &Type {
    if lending(d) { &Type::Unit } else { &d.result }
}

// Dispatch only. The raw definition and each actual entry are independently
// inspected below; a result head supplies no capture or input authority.
fn borrowed_owned_candidate(d: &ClosureDefinition) -> bool {
    d.storage_life.is_none()
        && d.lending_captures.is_empty()
        && d.kind != CallableKind::FunctionPointer
        && matches!(
            d.result,
            Type::Package { .. } | Type::Enum { .. } | Type::ErrorUnion { .. }
        )
}

fn callback_data_without_targets(ty: &Type, b: &mut Budget) -> Check<()> {
    super::stored_flow::type_work(ty, &b.limits.clone(), b)?;
    let mut pending = vec![ty];
    while let Some(current) = pending.pop() {
        b.spend(1)?;
        match current {
            Type::Address { .. }
            | Type::Desc(_)
            | Type::BorrowedStorage { .. }
            | Type::Closure(_)
            | Type::Opaque { .. }
            | Type::TaskI32
            | Type::Task { .. }
            | Type::Registration { .. }
            | Type::AsyncRegistration { .. } => {
                return Err(bad(
                    "stored owning callback data contains a target-bearing or callable environment dependency",
                ));
            }
            Type::Managed(inner)
            | Type::OwnedStore { element: inner }
            | Type::Array { element: inner, .. }
            | Type::Slice { element: inner } => {
                b.spend(1)?;
                pending.push(inner);
            }
            Type::Tuple(fields) => {
                b.spend(fields.len())?;
                pending.extend(fields);
            }
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                b.spend(fields.len())?;
                pending.extend(fields.iter().map(|(_, child)| child));
            }
            Type::Enum { variants, .. } => {
                b.spend(variants.len())?;
                for (_, fields) in variants {
                    b.spend(fields.len())?;
                    pending.extend(fields);
                }
            }
            Type::ErrorUnion { error, result } => {
                b.spend(2)?;
                pending.extend([error.as_ref(), result.as_ref()]);
            }
            Type::FunctionPointer { parameters, result }
            | Type::BorrowedFunctionPointer {
                parameters, result, ..
            }
            | Type::ForeignCallback { parameters, result } => {
                b.spend(parameters.len().saturating_add(1))?;
                pending.extend(parameters);
                pending.push(result);
            }
            _ => {}
        }
    }
    Ok(())
}

fn borrowed_owned_definition(
    module: &TypedModule,
    d: &ClosureDefinition,
    b: &mut Budget,
) -> Check<()> {
    resources::charge_payload(&d.result, b)?;
    super::borrowed_fp::signature_work(&d.parameters, &d.result, b)?;
    b.spend(d.parameters.len())?;
    callback_data_without_targets(&d.result, b)?;
    for parameter in &d.parameters {
        callback_data_without_targets(
            match parameter {
                Type::Address { inner, .. } => inner,
                other => other,
            },
            b,
        )?;
    }
    if !super::owned_callbacks::root(&d.result)
        || !d
            .parameters
            .iter()
            .any(|ty| matches!(ty, Type::Address { .. }))
        || !super::owned_callbacks::signature(module, d, b)?
    {
        return Err(bad(
            "stored owning callback requires an actual borrowed input and closed owned result",
        ));
    }
    Ok(())
}

// Masked stored proofs must authenticate an observed owning callback's actual
// body entry even when the surrounding module has no Option-result profile.
pub(super) fn owned_entry(
    module: &TypedModule,
    d: &ClosureDefinition,
    b: &mut Budget,
) -> Check<()> {
    if !borrowed_owned_candidate(d) {
        return Ok(());
    }
    b.spend(d.captures.len())?;
    if !d.captures.iter().all(scalar) {
        return Err(bad(
            "stored owning callback requires an actual scalar capture environment",
        ));
    }
    borrowed_owned_definition(module, d, b)?;
    super::borrowed_fp::signature_work(&d.parameters, &d.result, b)?;
    let f = module
        .functions
        .get(d.function.0 as usize)
        .ok_or_else(|| bad("stored owning callback actual entry is absent"))?;
    b.spend(f.parameters.len())?;
    if f.id != d.function
        || f.environment != Some(d.id)
        || f.foreign.is_some()
        || f.call_kind != CallKind::Direct
        || f.storage_life.is_some()
        || f.callable_contract.is_some()
        || f.callable_return_contract.is_some()
        || f.reference_return_bound.is_some()
        || f.result_mode != PassingMode::Infer
        || f.parameters.len() != d.parameters.len()
        || f.return_type != d.result
    {
        return Err(bad("stored owning callback actual entry differs"));
    }
    for (parameter, ty) in f.parameters.iter().zip(&d.parameters) {
        if f.locals.get(parameter.0 as usize).map(|local| &local.ty) != Some(ty) {
            return Err(bad("stored owning callback actual parameter differs"));
        }
    }
    Ok(())
}

pub(super) fn finite(module: &TypedModule, id: zil_ir::ClosureId) -> bool {
    module.closures.get(id.0 as usize).is_some_and(|d| {
        (d.lending_captures.is_empty() && scalar(&d.result))
            || lending(d)
            || borrowed_owned_candidate(d)
    })
}
pub(super) fn shared_input(t: &Type) -> bool {
    if scalar(t) {
        return true;
    }
    let Type::Address {
        mutable: false,
        inner,
    } = t
    else {
        return false;
    };
    match inner.as_ref() {
        Type::BorrowedStorage { storage, life } => {
            matches!(storage.as_ref(), Type::Record { .. } | Type::Tuple(_))
                && life.retained.is_empty()
                && life
                    .slots
                    .iter()
                    .all(|s| s.mode == StoredBorrowMode::SharedAddress)
        }
        t => t.is_address_pointee(),
    }
}
pub(super) fn captured_profile(d: &ClosureDefinition, b: &mut Budget) -> Check<()> {
    b.spend(d.captures.len().saturating_add(d.parameters.len()))?;
    for ty in &d.parameters {
        super::stored_flow::type_work(ty, &b.limits.clone(), b)?;
        if !shared_input(ty) {
            return Err(unsupported(
                "captured Fixed requires shared/scalar parameters",
                None,
            ));
        }
    }
    for ty in &d.captures {
        super::stored_flow::type_work(ty, &b.limits.clone(), b)?;
        if !(shared_input(ty) || super::stored_owned::owner(ty)) {
            return Err(unsupported(
                "captured Fixed requires shared/scalar captures or a finite class-free genuine Once owner",
                None,
            ));
        }
    }
    if !((d.lending_captures.is_empty() && scalar(&d.result)) || lending(d)) {
        return Err(unsupported(
            "captured Fixed requires a scalar nonlending result",
            None,
        ));
    }
    Ok(())
}
pub(super) fn value(module: &TypedModule, t: &Type) -> bool {
    let t = if let Type::Address { inner, .. } = t {
        inner.as_ref()
    } else {
        t
    };
    matches!(t,Type::Closure(id)if finite(module,*id))
}
pub(super) fn definitions(
    module: &TypedModule,
    mask: Option<&[bool]>,
    b: &mut Budget,
) -> Check<()> {
    b.spend(module.closures.len())?;
    for d in &module.closures {
        // The verifier derives this scheduling mask from actual signatures
        // and bodies. Unrelated ordinary closures still pass the separate
        // whole-module closure, resource, effect and loan checks.
        if !borrowed_owned_candidate(d)
            && mask.is_some_and(|selected| {
                !selected
                    .get(d.function.0 as usize)
                    .copied()
                    .unwrap_or(false)
            })
        {
            continue;
        }
        b.spend(d.captures.len())?;
        if !finite(module, d.id) || d.storage_life.is_none() && !d.captures.iter().all(scalar) {
            return Err(unsupported(
                "qualified callbacks require actual inline scalar environments and results",
                None,
            ));
        }
        if borrowed_owned_candidate(d) {
            borrowed_owned_definition(module, d, b)?;
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
        if let Some(c) = &d.storage_life {
            captured_profile(d, b)?;
            super::signature_life::charge(c, b)?;
            super::borrowed_fp::signature_work(&d.captures, &Type::Unit, b)?;
            if let Some(entry) = &f.storage_life {
                super::signature_life::charge(entry, b)?;
            }
            if f.storage_life.as_ref() != Some(c) {
                return Err(bad(
                    "captured Fixed definition and actual entry relation differ",
                ));
            }
            super::signature_life::closure_relation(
                &d.parameters,
                &d.captures,
                life_result(d),
                c,
                Some(f.span),
            )?;
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
        InstructionKind::BorrowCapture { mutable: false, .. } => f
            .environment
            .and_then(|id| module.closures.get(id.0 as usize))
            .is_some_and(lending),
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
        let d = module
            .closures
            .get(closure.0 as usize)
            .ok_or_else(|| bad("qualified environment definition is absent"))?;
        b.spend(captures.len())?;
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
        InstructionKind::BorrowCapture {
            index,
            mutable: false,
        } => {
            let d = f
                .environment
                .and_then(|id| module.closures.get(id.0 as usize))
                .filter(|d| lending(d))
                .ok_or_else(|| bad("capture address lacks a real lending environment"))?;
            let payload = d
                .captures
                .get(*index as usize)
                .filter(|ty| scalar(ty))
                .ok_or_else(|| {
                    bad("capture address must name an inline scalar environment field")
                })?;
            super::stored_flow::type_work(payload, &b.limits.clone(), b)?;
            let result = i.result.and_then(|id| f.values.get(id.0 as usize));
            if !matches!(result,Some(Type::Address {mutable:false,inner}) if inner.as_ref()==payload)
            {
                return Err(bad(
                    "capture address actual result differs from its environment field",
                ));
            }
            return Ok(());
        }
        InstructionKind::ReadCapture { index } | InstructionKind::WriteCapture { index, .. } => {
            let d = f
                .environment
                .and_then(|id| module.closures.get(id.0 as usize))
                .ok_or_else(|| bad("qualified capture access lacks actual environment"))?;
            let t = d
                .captures
                .get(*index as usize)
                .ok_or_else(|| bad("qualified capture access lacks actual field"))?;
            if let InstructionKind::WriteCapture { value, .. } = &i.kind {
                if d.storage_life.is_some() && !scalar(t) {
                    return Err(unsupported(
                        "captured Fixed mutation is limited to inline scalar fields",
                        Some(i.span),
                    ));
                }
                if d.kind == zil_ir::CallableKind::Fn
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
    if d.storage_life.is_some()
        && matches!(i.kind, InstructionKind::CallClosureOnce { .. })
        && d.kind != zil_ir::CallableKind::FnOnce
        && (!matches!(
            d.kind,
            zil_ir::CallableKind::Fn | zil_ir::CallableKind::FnMut
        ) || !scalar(&d.result)
            || !d.lending_captures.is_empty())
    {
        return Err(unsupported(
            "captured Fixed Once adaptation requires an actual nonlending scalar entry",
            Some(i.span),
        ));
    }
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
    module: &TypedModule,
    f: &'a TypedFunction,
    kind: &InstructionKind,
    definition: &ClosureDefinition,
    b: &mut Budget,
) -> Check<Option<&'a StorageLifeContract>> {
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
    b.spend(f.parameters.len().saturating_add(contract.roles.len()))?;
    let direct = f.parameters.iter().position(|p| *p == local);
    let position = match direct {
        Some(position) => position,
        None if matches!(kind, InstructionKind::CallClosureOnce { .. }) => {
            staged::parameter(module, f, kind, definition, b)?
        }
        None => {
            return Err(bad(
                "callback role receiver is not an actual declared parameter",
            ));
        }
    };
    let role = contract
        .roles
        .iter()
        .find(|r| r.parameter as usize == position)
        .ok_or_else(|| bad("callback receiver has no exact parameter role"))?;
    if direct.is_none() && (role.closure != definition.id || role.cleanup != CallableCleanup::Owned)
    {
        return Err(bad(
            "staged callback requires the exact owned parameter role",
        ));
    }
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

pub(super) fn roles(module: &TypedModule, graphs: &[FunctionGraph], b: &mut Budget) -> Check<()> {
    if module.closures.is_empty() {
        return Ok(());
    }
    b.spend(module.functions.len())?;
    for f in &module.functions {
        if let Some(contract) = &f.callable_contract {
            b.spend(contract.roles.len())?;
            for role in &contract.roles {
                if !finite(module, role.closure) {
                    return Err(bad(
                        "qualified role requires actual finite environment entry",
                    ));
                }
                let d = &module.closures[role.closure.0 as usize];
                b.spend(d.parameters.len())?;
                let needs=d.parameters.iter().any(|t|matches!(t,Type::BorrowedStorage{..})||matches!(t,Type::Address{inner,..}if matches!(inner.as_ref(),Type::BorrowedStorage{..})));
                if let Some(life) = &role.life {
                    let entry = &module.functions[d.function.0 as usize];
                    if let Some(definition) = &d.storage_life {
                        super::signature_life::charge(life, b)?;
                        super::signature_life::closure_relation(
                            &d.parameters,
                            &d.captures,
                            life_result(d),
                            life,
                            Some(entry.span),
                        )?;
                        if life != definition {
                            return Err(unsupported(
                                "captured Fixed role adaptation requires its actual capture map",
                                Some(entry.span),
                            ));
                        }
                    }
                    super::class_flow::check_role(
                        module,
                        entry,
                        &graphs[d.function.0 as usize].reachable,
                        life,
                        b,
                    )?;
                } else if needs || d.storage_life.is_some() {
                    return Err(bad(
                        "qualified role omits its complete semantic class relation",
                    ));
                }
            }
        }
    }
    Ok(())
}
