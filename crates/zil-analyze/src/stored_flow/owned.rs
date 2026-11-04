//! Bounded ownership composition, not stored-address permission. Actual debt,
//! capture consumption, weak-description liveness and Drop remain resource proof.
use super::*;

fn closed(text: &str) -> Vec<Diagnostic> {
    vec![Diagnostic::unsupported(Phase::Analyze, text, None)]
}

pub(super) fn data(ty: &Type, b: &mut Budget) -> CompileResult<bool> {
    match ty {
        Type::Managed(payload) | Type::Desc(payload) => {
            super::super::stored_callables::resource_payload(payload, b)
        }
        _ => Ok(false),
    }
}

fn value(ty: &Type, b: &mut Budget) -> CompileResult<()> {
    fee(b, 1)?;
    let mut pending = vec![(ty, true, 0usize)];
    while let Some((ty, top, depth)) = pending.pop() {
        fee(b, 1)?;
        if depth > b.limits.max_call_depth {
            return Err(vec![Diagnostic::limit(
                Phase::Analyze,
                "owned Fixed value depth exceeded",
                None,
            )]);
        }
        match ty {
            Type::Managed(_) | Type::Desc(_) if top && data(ty, b)? => {}
            Type::Closure(_) if top => {}
            Type::Address { inner, .. } if top && matches!(inner.as_ref(), Type::Closure(_)) => {}
            Type::Managed(_)
            | Type::Desc(_)
            | Type::Package { .. }
            | Type::Closure(_)
            | Type::ByteBuffer
            | Type::OwnedStore { .. }
            | Type::Task { .. }
            | Type::TaskI32
            | Type::Registration { .. }
            | Type::AsyncRegistration { .. } => {
                return Err(closed(
                    "owned Fixed permits only direct class-free resource heads, not nested owning/address heads",
                ));
            }
            Type::Address { inner, .. }
            | Type::Array { element: inner, .. }
            | Type::Slice { element: inner }
            | Type::Opaque { hidden: inner, .. }
            | Type::BorrowedStorage { storage: inner, .. } => {
                fee(b, 1)?;
                pending.push((inner, false, depth + 1));
            }
            Type::Record { fields, .. } => {
                fee(b, fields.len())?;
                pending.extend(fields.iter().map(|(_, t)| (t, false, depth + 1)));
            }
            Type::Tuple(fields) => {
                fee(b, fields.len())?;
                pending.extend(fields.iter().map(|t| (t, false, depth + 1)));
            }
            Type::Enum { variants, .. } => {
                fee(b, variants.len())?;
                for (_, fields) in variants {
                    fee(b, fields.len())?;
                    pending.extend(fields.iter().map(|t| (t, false, depth + 1)));
                }
            }
            Type::ErrorUnion { error, result } => {
                fee(b, 2)?;
                pending.extend([
                    (error.as_ref(), false, depth + 1),
                    (result.as_ref(), false, depth + 1),
                ]);
            }
            Type::FunctionPointer { parameters, result }
            | Type::BorrowedFunctionPointer {
                parameters, result, ..
            } => {
                fee(b, parameters.len().saturating_add(1))?;
                pending.extend(parameters.iter().map(|t| (t, false, depth + 1)));
                pending.push((result, false, depth + 1));
            }
            _ => {}
        }
    }
    Ok(())
}

pub(super) fn profile(module: &TypedModule, b: &mut Budget) -> CompileResult<bool> {
    if module.resources.is_empty() {
        return Ok(false);
    }
    fee(b, module.closures.len())?;
    let mut candidates = vec![false; module.closures.len()];
    for (index, d) in module.closures.iter().enumerate() {
        fee(b, d.captures.len().saturating_add(1))?;
        if d.kind == CallableKind::FnOnce && d.storage_life.is_some() {
            for capture in &d.captures {
                candidates[index] |= super::super::stored_callables::owned_capture(capture, b)?;
            }
        }
    }
    let mut actual = false;
    if candidates.contains(&true) {
        for i in module
            .functions
            .iter()
            .flat_map(|f| &f.blocks)
            .flat_map(|b| &b.instructions)
        {
            fee(b, 1)?;
            if let InstructionKind::MakeClosure { closure, .. } = i.kind {
                actual |= candidates.get(closure.0 as usize).copied().unwrap_or(false);
            }
        }
    }
    if !actual {
        return Err(closed(
            "owned/closure storage integration requires an actual bounded owning Fixed environment",
        ));
    }
    fee(b, module.resources.len())?;
    for r in &module.resources {
        b.type_work(&r.payload, true, "bounded resource definition payload")?;
        if !super::super::stored_callables::resource_payload(&r.payload, b)? {
            return Err(closed(
                "owned Fixed requires class-free scalar-field resource payloads",
            ));
        }
    }
    for f in &module.functions {
        fee(b, f.parameters.len().saturating_add(1))?;
        if matches!(f.return_type, Type::Managed(_) | Type::Desc(_)) {
            return Err(closed(
                "owned Fixed resource result transfer is outside this profile",
            ));
        }
        let teardown = module.resources.iter().any(|r| r.teardown == Some(f.id));
        fee(b, module.resources.len())?;
        for p in &f.parameters {
            let ty = &f.locals[p.0 as usize].ty;
            if matches!(ty, Type::Managed(_)) || matches!(ty, Type::Desc(_)) && !teardown {
                return Err(closed(
                    "owned Fixed permits resource description parameters only at actual teardown entries",
                ));
            }
        }
        for ty in f
            .locals
            .iter()
            .map(|l| &l.ty)
            .chain(&f.values)
            .chain(std::iter::once(&f.return_type))
        {
            value(ty, b)?;
        }
    }
    Ok(true)
}

pub(super) fn describe(out: &mut Value, base: HoldRoot, b: &mut Budget) -> CompileResult<()> {
    // Weak resource identity is not a stored pointer or a parent borrow loan.
    fee(b, 1)?;
    out.held.targets.insert(Target {
        base,
        stored: Vec::new(),
        place: Vec::new(),
    });
    Ok(())
}

pub(super) fn operation(kind: &InstructionKind) -> bool {
    matches!(
        kind,
        InstructionKind::MakeManaged { .. }
            | InstructionKind::TakeCapture { .. }
            | InstructionKind::DescribeLocal { .. }
            | InstructionKind::DescribeCapture { .. }
            | InstructionKind::ReadDesc { .. }
            | InstructionKind::DropManaged { .. }
    )
}
fn result(
    f: &TypedFunction,
    i: &Instruction,
    expected: &Type,
    b: &mut Budget,
) -> CompileResult<()> {
    b.type_work(expected, false, "owned Fixed exact producer result")?;
    let id = i
        .result
        .ok_or_else(|| failure("resource producer has no result"))?;
    if value_type(f, id)? != expected {
        return Err(failure("resource producer result differs from actual type"));
    }
    Ok(())
}
fn payload<'a>(ty: &'a Type, managed: bool, b: &mut Budget) -> CompileResult<&'a Type> {
    let p = match (managed, ty) {
        (true, Type::Managed(p)) | (false, Type::Desc(p)) => p.as_ref(),
        _ => {
            return Err(failure(
                "resource operation requires its exact head/description kind",
            ));
        }
    };
    if !super::super::stored_callables::resource_payload(p, b)? {
        return Err(closed(
            "resource operation needs a class-free scalar-field payload",
        ));
    }
    Ok(p)
}
fn capture<'a>(
    module: &'a TypedModule,
    f: &TypedFunction,
    index: u32,
) -> CompileResult<(&'a ClosureDefinition, &'a Type)> {
    let d = f
        .environment
        .and_then(|id| module.closures.get(id.0 as usize))
        .ok_or_else(|| failure("resource capture has no actual environment"))?;
    let t = d
        .captures
        .get(index as usize)
        .ok_or_else(|| failure("resource capture is absent"))?;
    Ok((d, t))
}

pub(super) fn instruction(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    b: &mut Budget,
) -> CompileResult<bool> {
    match &i.kind {
        InstructionKind::MakeManaged { fields } => {
            let ty = value_type(
                f,
                i.result
                    .ok_or_else(|| failure("resource construction lacks result"))?,
            )?;
            let Type::Record {
                fields: declared, ..
            } = payload(ty, true, b)?
            else {
                unreachable!()
            };
            fee(b, fields.len())?;
            if fields.len() != declared.len() {
                return Err(failure(
                    "resource constructor fields differ from actual payload",
                ));
            }
            for (v, (_, expected)) in fields.iter().zip(declared) {
                if value_type(f, *v)? != expected {
                    return Err(failure(
                        "resource constructor field has the wrong actual type",
                    ));
                }
            }
        }
        InstructionKind::TakeCapture { index } => {
            let (d, ty) = capture(module, f, *index)?;
            if d.kind != CallableKind::FnOnce {
                return Err(failure(
                    "owned capture transfer requires a true FnOnce definition",
                ));
            }
            payload(ty, true, b)?;
            result(f, i, ty, b)?;
        }
        InstructionKind::TakeLocal { local }
            if matches!(f.locals[local.0 as usize].ty, Type::Managed(_)) =>
        {
            let ty = &f.locals[local.0 as usize].ty;
            payload(ty, true, b)?;
            result(f, i, ty, b)?;
        }
        InstructionKind::DescribeLocal { local } => {
            let ty = f
                .locals
                .get(local.0 as usize)
                .ok_or_else(|| failure("described resource local is absent"))?;
            let p = payload(&ty.ty, true, b)?;
            let out = value_type(
                f,
                i.result
                    .ok_or_else(|| failure("resource description lacks result"))?,
            )?;
            b.type_work(p, false, "resource description payload comparison")?;
            if !matches!(out, Type::Desc(inner) if inner.as_ref()==p) {
                return Err(failure(
                    "resource description differs from actual local payload",
                ));
            }
        }
        InstructionKind::DescribeCapture { index } => {
            let (_, ty) = capture(module, f, *index)?;
            let p = payload(ty, true, b)?;
            let out = value_type(
                f,
                i.result
                    .ok_or_else(|| failure("resource description lacks result"))?,
            )?;
            b.type_work(p, false, "resource capture description payload comparison")?;
            if !matches!(out, Type::Desc(inner) if inner.as_ref()==p) {
                return Err(failure(
                    "resource description differs from actual capture payload",
                ));
            }
        }
        InstructionKind::ReadDesc { desc, index } => {
            let Type::Record { fields, .. } = payload(value_type(f, *desc)?, false, b)? else {
                unreachable!()
            };
            let ty = &fields
                .get(*index as usize)
                .ok_or_else(|| failure("resource field projection is absent"))?
                .1;
            result(f, i, ty, b)?;
        }
        InstructionKind::DropManaged { local } => {
            let ty = f
                .locals
                .get(local.0 as usize)
                .ok_or_else(|| failure("dropped resource local is absent"))?;
            payload(&ty.ty, true, b)?;
            if i.result.is_some() {
                return Err(failure("resource drop cannot produce a value"));
            }
        }
        _ => return Ok(false),
    }
    Ok(true)
}
