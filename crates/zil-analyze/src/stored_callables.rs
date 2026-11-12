//! Qualified callbacks with inline scalar environments. Metadata is checked
//! against the actual definition and receiver; roles use parameter positions.
use super::*;
mod foreign;
mod staged;
pub(super) use foreign::regions as foreign_regions;

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
            | Type::ForeignHandle
    )
}
pub(super) fn scalar_foreign(f: &TypedFunction, b: &mut Budget) -> CompileResult<bool> {
    foreign::declaration(f, b)
}

pub(super) fn lending(d: &ClosureDefinition) -> bool {
    d.storage_life.is_some()
        && !d.lending_captures.is_empty()
        && d.kind != CallableKind::FnOnce
        && matches!(&d.result, Type::Address { mutable:false, inner } if scalar(inner))
}
pub(super) fn life_result(d: &ClosureDefinition) -> &Type {
    if lending(d)
        && !d.storage_life.as_ref().is_some_and(|life| {
            life.bindings
                .iter()
                .any(|row| row.position.base == StorageBase::Result)
        })
    {
        &Type::Unit
    } else {
        &d.result
    }
}
pub(super) fn result_is_receiver(d: &ClosureDefinition) -> bool {
    d.storage_life.as_ref().is_some_and(|life| {
        super::signature_life::outer(life, StorageBase::Receiver).is_some_and(|head| {
            super::signature_life::outer(life, StorageBase::Result) == Some(head)
        })
    })
}
pub(super) fn fixed_result(d: &ClosureDefinition) -> bool {
    d.storage_life.as_ref().is_some_and(|life| {
        matches!(
            super::signature_life::outer(life, StorageBase::Result),
            Some(StorageBinder::Fixed(_))
        )
    })
}

pub(super) fn transferred(d: &ClosureDefinition) -> bool {
    d.kind == CallableKind::FnOnce
        && matches!(&d.result, Type::Address { inner, .. } if inner.is_address_pointee())
        && fixed_result(d)
        && !d.storage_life.as_ref().is_some_and(|life| {
            life.bindings
                .iter()
                .any(|row| row.position.base == StorageBase::Receiver)
        })
}

// A scheduling tag, never qualification. definitions() rechecks the complete
// actual inputs, result, captures and entry before this ledger uses the value.
fn borrowed_owned_candidate(d: &ClosureDefinition) -> bool {
    d.storage_life.is_none()
        && d.lending_captures.is_empty()
        && d.kind != CallableKind::FunctionPointer
        && matches!(
            d.result,
            Type::Package { .. } | Type::Enum { .. } | Type::ErrorUnion { .. }
        )
}

fn closed_callback_data(ty: &Type, b: &mut Budget) -> CompileResult<()> {
    b.type_work(ty, false, "stored callback closed target-free data")?;
    fn closed(ty: &Type) -> bool {
        match ty {
            Type::Address { .. }
            | Type::Desc(_)
            | Type::BorrowedStorage { .. }
            | Type::Closure(_)
            | Type::Opaque { .. }
            | Type::TaskI32
            | Type::Task { .. }
            | Type::Registration { .. }
            | Type::AsyncRegistration { .. } => false,
            Type::Managed(inner)
            | Type::OwnedStore { element: inner }
            | Type::Array { element: inner, .. }
            | Type::Slice { element: inner } => closed(inner),
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                fields.iter().all(|(_, child)| closed(child))
            }
            Type::Tuple(fields) => fields.iter().all(closed),
            Type::Enum { variants, .. } => {
                variants.iter().all(|(_, fields)| fields.iter().all(closed))
            }
            Type::ErrorUnion { error, result } => closed(error) && closed(result),
            Type::FunctionPointer { parameters, result }
            | Type::BorrowedFunctionPointer {
                parameters, result, ..
            }
            | Type::ForeignCallback { parameters, result } => {
                parameters.iter().all(closed) && closed(result)
            }
            _ => true,
        }
    }
    if !closed(ty) {
        return Err(bad(
            "stored owning callback data contains a target-bearing or callable environment dependency",
        ));
    }
    Ok(())
}

fn borrowed_owned_definition(
    module: &TypedModule,
    d: &ClosureDefinition,
    b: &mut Budget,
) -> CompileResult<()> {
    b.type_work(&d.result, true, "stored callback actual owned result")?;
    b.spend(
        d.parameters.len(),
        "stored callback actual borrowed input inventory",
    )?;
    if d.result.is_copy_value()
        || !d
            .parameters
            .iter()
            .any(|ty| matches!(ty, Type::Address { .. }))
    {
        return Err(bad(
            "stored owning callback requires an actual borrowed input and closed owned result",
        ));
    }
    closed_callback_data(&d.result, b)?;
    super::owned_callbacks::data(module, &d.result, b)?;
    for ty in &d.parameters {
        let pointee = if let Type::Address { inner, .. } = ty {
            inner.as_ref()
        } else {
            ty
        };
        closed_callback_data(pointee, b)?;
        super::owned_callbacks::input(module, ty, b)?;
    }
    if module
        .functions
        .get(d.function.0 as usize)
        .is_some_and(|entry| entry.reference_return_bound.is_some())
    {
        return Err(bad(
            "stored owning callback cannot claim a borrowed result bound",
        ));
    }
    super::owned_callbacks::signature(module, d, b)
}

pub(super) fn finite(module: &TypedModule, id: zil_ir::ClosureId) -> bool {
    module.closures.get(id.0 as usize).is_some_and(|d| {
        (d.lending_captures.is_empty() && scalar(&d.result))
            || lending(d)
            || transferred(d)
            || borrowed_owned_candidate(d)
    })
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
/// Reborrow the head on every read, preserving its nominal pointee classes.
pub(super) fn mutable_context_capture(t: &Type) -> bool {
    matches!(t, Type::Address { mutable: true, inner }
        if matches!(inner.as_ref(), Type::BorrowedStorage { storage, life }
            if matches!(storage.as_ref(), Type::Record { .. } | Type::Tuple(_))
                && life.retained.is_empty()))
}
pub(super) fn exclusive_capture(t: &Type) -> bool {
    mutable_context_capture(t)
        || matches!(t, Type::Address { mutable: true, inner } if scalar(inner))
}
pub(super) fn resource_payload(t: &Type, b: &mut Budget) -> CompileResult<bool> {
    let Type::Record { fields, .. } = t else {
        return Ok(false);
    };
    b.spend(
        fields.len().saturating_add(1),
        "class-free resource payload eligibility",
    )?;
    Ok(fields.iter().all(|(_, t)| scalar(t)))
}
pub(super) fn owned_capture(t: &Type, b: &mut Budget) -> CompileResult<bool> {
    let Type::Managed(payload) = t else {
        return Ok(false);
    };
    resource_payload(payload, b)
}
pub(super) fn definitions(module: &TypedModule, b: &mut Budget) -> CompileResult<()> {
    b.spend(module.closures.len(), "qualified callback inventory")?;
    for d in &module.closures {
        let borrowed_owned = borrowed_owned_candidate(d);
        if !borrowed_owned
            && !b.composition_stored.is_empty()
            && !b
                .composition_stored
                .get(d.function.0 as usize)
                .copied()
                .unwrap_or(false)
        {
            continue;
        }
        b.spend(d.captures.len(), "qualified scalar capture inventory")?;
        let mut admitted = finite(module, d.id);
        for t in &d.captures {
            if matches!(t, Type::BorrowedStorage { storage, .. } if matches!(storage.as_ref(), Type::Package { .. }))
                && b.returned_borrowed_environments.contains(&d.id)
            {
                super::resources::product::admit_owned(module, t, b)?;
                continue;
            }
            admitted &= if d.storage_life.is_some() {
                capture(t)
                    || exclusive_capture(t)
                    || owned_capture(t, b)?
                    || matches!(t,Type::Closure(child) if b.returned_borrowed_environments.contains(child) && finite(module,*child))
            } else {
                scalar(t)
                    || capture(t)
                    || exclusive_capture(t)
                    || b.returned_borrowed_environments.contains(&d.id)
                        && (owned_capture(t, b)?
                            || matches!(t, Type::Closure(child)
                                if b.returned_borrowed_environments.contains(child)
                                    && finite(module, *child)))
            };
        }
        if !admitted {
            return Err(vec![Diagnostic::unsupported(
                Phase::Analyze,
                "qualified callbacks require finite scalar/shared capture environments and scalar results",
                None,
            )]);
        }
        if borrowed_owned {
            borrowed_owned_definition(module, d, b)?;
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
            super::borrowed_fp::qualified_array_relation_work(&d.parameters, &d.result, b)?;
            super::borrowed_fp::qualified_array_relation_work(&d.captures, &Type::Unit, b)?;
            super::signature_life::closure_relation(
                &module.closures,
                d.kind,
                &d.parameters,
                &d.captures,
                life_result(d),
                life,
            )?;
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
        InstructionKind::BorrowCapture { index, mutable } => f
            .environment
            .and_then(|id| module.closures.get(id.0 as usize))
            .is_some_and(|d| {
                (!*mutable && lending(d))
                    || d.captures.get(*index as usize).is_some_and(
                        |ty| matches!(ty, Type::Closure(child) if finite(module,*child)),
                    )
            }),
        InstructionKind::TakeCapture { index } => f
            .environment
            .and_then(|id| module.closures.get(id.0 as usize))
            .and_then(|d| d.captures.get(*index as usize))
            .is_some_and(|ty| {
                matches!(ty,Type::Closure(child) if finite(module,*child)) || exclusive_capture(ty)
            }),
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
        && (!matches!(d.kind, CallableKind::Fn | CallableKind::FnMut)
            || !(scalar(&d.result) || fixed_result(d))
            || !d.lending_captures.is_empty() && !fixed_result(d))
    {
        return Err(vec![Diagnostic::unsupported(
            Phase::Analyze,
            "captured Fixed Once adaptation requires an actual nonlending scalar entry",
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
    if let InstructionKind::BorrowCapture { index, mutable } = i.kind
        && let Some(definition) = f
            .environment
            .and_then(|id| module.closures.get(id.0 as usize))
        && b.returned_borrowed_environments.contains(&definition.id)
        && let Some(Type::Closure(child)) = definition.captures.get(index as usize)
        && b.returned_borrowed_environments.contains(child)
    {
        if mutable && definition.kind == zil_ir::CallableKind::Fn {
            return Err(bad(
                "nested exclusive capture requires an exclusive environment entry",
            ));
        }
        if !matches!(i.result.and_then(|value|f.values.get(value.0 as usize)),
            Some(Type::Address { mutable: actual, inner }) if *actual==mutable && inner.as_ref()==&Type::Closure(*child))
        {
            return Err(bad(
                "nested capture borrow differs from its actual environment field",
            ));
        }
        return Ok(());
    }
    if let InstructionKind::TakeCapture { index } = i.kind
        && let Some(definition) = f
            .environment
            .and_then(|id| module.closures.get(id.0 as usize))
        && b.returned_borrowed_environments.contains(&definition.id)
        && let Some(Type::Closure(child)) = definition.captures.get(index as usize)
        && b.returned_borrowed_environments.contains(child)
    {
        if definition.kind != zil_ir::CallableKind::FnOnce
            || i.result.and_then(|value| f.values.get(value.0 as usize))
                != Some(&Type::Closure(*child))
        {
            return Err(bad(
                "nested capture transfer requires its actual consuming environment entry",
            ));
        }
        return Ok(());
    }
    match &i.kind {
        InstructionKind::BorrowCapture {
            index,
            mutable: false,
        } => {
            let definition = f
                .environment
                .and_then(|id| module.closures.get(id.0 as usize))
                .filter(|d| lending(d))
                .ok_or_else(|| bad("capture loan requires an actual lending entry"))?;
            let actual = definition
                .captures
                .get(*index as usize)
                .filter(|ty| scalar(ty))
                .ok_or_else(|| bad("capture loan requires a real inline scalar field"))?;
            b.type_work(actual, false, "inline capture loan actual payload")?;
            if !matches!(i.result.and_then(|v|f.values.get(v.0 as usize)),Some(Type::Address {mutable:false,inner}) if inner.as_ref()==actual)
            {
                return Err(bad("inline capture loan has a different actual result"));
            }
            return Ok(());
        }
        InstructionKind::TakeCapture { index } => {
            let d = f
                .environment
                .and_then(|id| module.closures.get(id.0 as usize))
                .ok_or_else(|| bad("capture transfer has no actual entry"))?;
            let ty = d
                .captures
                .get(*index as usize)
                .ok_or_else(|| bad("capture transfer field absent"))?;
            if d.kind != CallableKind::FnOnce
                || !exclusive_capture(ty)
                || i.result.and_then(|id| f.values.get(id.0 as usize)) != Some(ty)
            {
                return Err(bad(
                    "borrowed capture capability transfer requires its genuine consuming entry and exact type",
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
            } else if i.result.and_then(|v| f.values.get(v.0 as usize)) != Some(t)
                && !(d.kind == CallableKind::Fn
                    && matches!((t, i.result.and_then(|v| f.values.get(v.0 as usize))),
                        (Type::Address { mutable: true, inner },
                         Some(Type::Address { mutable: false, inner: result })) if inner == result))
            {
                return Err(bad(
                    "qualified capture read result differs from actual field",
                ));
            }
            if d.kind == CallableKind::Fn
                && exclusive_capture(t)
                && matches!(
                    i.result.and_then(|v| f.values.get(v.0 as usize)),
                    Some(Type::Address { mutable: true, .. })
                )
            {
                return Err(bad(
                    "shared environment cannot expose a mutable captured capability",
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
    module: &TypedModule,
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
    let position = match f.parameters.iter().position(|p| *p == local) {
        Some(position) => position,
        None => {
            b.lookup(
                b.callable_receiver_roles.len(),
                "proved callback receiver role site",
            )?;
            if let Some(position) = b
                .callable_receiver_roles
                .get(&(f.id, kind as *const InstructionKind as usize))
            {
                *position as usize
            } else {
                staged::parameter(module, f, kind, b)?
            }
        }
    };
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
        if !b.composition_stored.is_empty() && !b.composition_stored[f.id.0 as usize] {
            continue;
        }
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
