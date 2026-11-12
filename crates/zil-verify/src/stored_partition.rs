//! Bounded separation of actual stored-reference bodies from ordinary bodies.
//! This table is reconstructed from raw types and calls, never from a plan.
use super::*;

fn stored_type(ty: &Type, budget: &mut Budget) -> Check<bool> {
    let mut pending = vec![ty];
    let mut found = false;
    while let Some(ty) = pending.pop() {
        budget.spend(1)?;
        match ty {
            Type::BorrowedStorage { storage, .. } => {
                found = true;
                pending.push(storage);
            }
            Type::OwnedStore { element }
            | Type::Registration {
                environment: element,
            }
            | Type::AsyncRegistration {
                environment: element,
            }
            | Type::Task {
                result: element, ..
            } => pending.push(element),
            Type::FunctionPointer { parameters, result }
            | Type::BorrowedFunctionPointer {
                parameters, result, ..
            }
            | Type::ForeignCallback { parameters, result } => {
                pending.extend(parameters);
                pending.push(result);
            }
            Type::Address { inner, .. }
            | Type::Managed(inner)
            | Type::Desc(inner)
            | Type::Array { element: inner, .. }
            | Type::Slice { element: inner }
            | Type::Opaque { hidden: inner, .. } => pending.push(inner),
            Type::Tuple(fields) => pending.extend(fields),
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                pending.extend(fields.iter().map(|(_, ty)| ty));
            }
            Type::Enum { variants, .. } => {
                pending.extend(variants.iter().flat_map(|(_, fields)| fields));
            }
            Type::ErrorUnion { error, result } => pending.extend([error.as_ref(), result.as_ref()]),
            _ => {}
        }
    }
    Ok(found)
}

fn forbidden_stored_type(ty: &Type, budget: &mut Budget) -> Check<bool> {
    let mut pending = vec![ty];
    while let Some(ty) = pending.pop() {
        budget.spend(1)?;
        match ty {
            Type::Managed(_)
            | Type::Desc(_)
            | Type::Closure(_)
            | Type::OwnedStore { .. }
            | Type::Registration { .. }
            | Type::AsyncRegistration { .. }
            | Type::TaskI32
            | Type::Task { .. }
            | Type::ByteBuffer
            | Type::FunctionPointer { .. }
            | Type::BorrowedFunctionPointer { .. }
            | Type::ForeignCallback { .. } => return Ok(true),
            Type::BorrowedStorage { storage, .. }
            | Type::Address { inner: storage, .. }
            | Type::Array {
                element: storage, ..
            }
            | Type::Slice { element: storage }
            | Type::Opaque {
                hidden: storage, ..
            } => pending.push(storage),
            Type::Tuple(fields) => pending.extend(fields),
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                pending.extend(fields.iter().map(|(_, ty)| ty))
            }
            Type::Enum { variants, .. } => {
                pending.extend(variants.iter().flat_map(|(_, fields)| fields))
            }
            Type::ErrorUnion { error, result } => pending.extend([error.as_ref(), result.as_ref()]),
            _ => {}
        }
    }
    Ok(false)
}

fn pure_boundary(f: &TypedFunction) -> bool {
    f.call_kind == CallKind::Direct
        && f.foreign.is_none()
        && f.environment.is_none()
        && f.storage_life.is_none()
        && f.reference_return_bound.is_none()
        && f.callable_contract.is_none()
        && f.callable_return_contract.is_none()
        && f.protocol_contract.is_none()
        && f.return_type == Type::I32
        && f.parameters.len() == 1
        && f.locals
            .get(f.parameters[0].0 as usize)
            .is_some_and(|l| l.ty == Type::I32)
}

pub(super) fn mixed(module: &TypedModule, budget: &mut Budget) -> Check<Option<Vec<bool>>> {
    budget.spend(module.functions.len())?;
    let mut mask = Vec::with_capacity(module.functions.len());
    for f in &module.functions {
        budget.spend(
            f.locals
                .len()
                .saturating_add(f.values.len())
                .saturating_add(2),
        )?;
        let mut stored = f.storage_life.is_some()
            && f.call_kind != CallKind::Suspend
            && !tasks::borrowed_signature(module, f, budget)?;
        for ty in std::iter::once(&f.return_type)
            .chain(f.locals.iter().map(|l| &l.ty))
            .chain(&f.values)
        {
            stored |= stored_type(ty, budget)?;
        }
        mask.push(stored);
    }
    if !mask.iter().any(|v| *v) || mask.iter().all(|v| *v) {
        return Ok(None);
    }
    for definition in &module.closures {
        budget.spend(1)?;
        let Some(stored) = mask.get(definition.function.0 as usize) else {
            return Err(reject(
                "E-VERIFY-STORED-PARTITION",
                None,
                "closure entry is outside the function table",
            ));
        };
        if *stored {
            return Err(unsupported(
                "mixed stored domain cannot contain a closure entry",
                None,
            ));
        }
    }
    budget.spend(module.dyn_vtables.len())?;
    for table in &module.dyn_vtables {
        budget.spend(table.entries.len().saturating_add(1))?;
        for entry in table.entries.iter().chain(table.cleanup.iter()) {
            if mask.get(entry.function.0 as usize).copied().unwrap_or(true) {
                return Err(unsupported(
                    "mixed stored domain cannot contain a dynamic entry",
                    None,
                ));
            }
        }
    }
    for resource in &module.resources {
        budget.spend(1)?;
        if stored_type(&resource.payload, budget)? {
            return Err(unsupported(
                "mixed stored domain cannot contain resource payload",
                None,
            ));
        }
        if resource
            .teardown
            .is_some_and(|id| mask.get(id.0 as usize).copied().unwrap_or(true))
        {
            return Err(unsupported(
                "mixed stored domain cannot contain resource teardown",
                None,
            ));
        }
    }
    // This first mixed slice has no stored-domain owned carrier or closure.
    // Ordinary Task/Registration/resource definitions remain checked globally.
    for (index, f) in module.functions.iter().enumerate() {
        if mask[index] {
            let mut forbidden = f.environment.is_some();
            for ty in std::iter::once(&f.return_type)
                .chain(f.locals.iter().map(|l| &l.ty))
                .chain(&f.values)
            {
                forbidden |= forbidden_stored_type(ty, budget)?;
            }
            if forbidden {
                return Err(unsupported(
                    "mixed stored domain requires scalar holder bodies without owned carriers or closures",
                    Some(f.span),
                ));
            }
        }
        for block in &f.blocks {
            budget.spend(block.instructions.len())?;
            for i in &block.instructions {
                let target = match i.kind {
                    InstructionKind::Call { function, .. }
                    | InstructionKind::SuspendCall { function, .. }
                    | InstructionKind::ForeignCall { function, .. }
                    | InstructionKind::FunctionRef { function }
                    | InstructionKind::ForeignFunctionRef { function } => Some(function),
                    InstructionKind::TaskCreate { worker, .. } | InstructionKind::TaskCreateOwned { worker, .. }
                    | InstructionKind::TaskStart { worker, .. }
                    | InstructionKind::WithScopedThread { worker, .. } => Some(worker),
                    _ => None,
                };
                let Some(target) = target else { continue };
                let target = target.0 as usize;
                let Some(other) = module.functions.get(target) else {
                    return Err(reject(
                        "E-VERIFY-STORED-PARTITION",
                        Some(i.span),
                        "function edge target is absent",
                    ));
                };
                if mask[index] == mask[target] {
                    continue;
                }
                let InstructionKind::Call { arguments, .. } = &i.kind else {
                    return Err(unsupported(
                        "mixed stored/ordinary non-call function edge",
                        Some(i.span),
                    ));
                };
                budget.spend(arguments.len().saturating_add(2))?;
                if !pure_boundary(other)
                    || arguments.len() != other.parameters.len()
                    || arguments
                        .iter()
                        .any(|id| f.values.get(id.0 as usize) != Some(&Type::I32))
                    || i.result.and_then(|id| f.values.get(id.0 as usize))
                        != Some(&other.return_type)
                {
                    return Err(unsupported(
                        "mixed stored/ordinary edge requires actual direct pure scalar signature and operands",
                        Some(i.span),
                    ));
                }
            }
            for i in &block.instructions {
                let mut targets = Vec::new();
                match i.kind {
                    InstructionKind::WithForeignCallback {
                        register, close, ..
                    }
                    | InstructionKind::RegistrationStart {
                        register, close, ..
                    } => targets.extend([register, close]),
                    InstructionKind::AsyncRegistrationStart {
                        register,
                        request_close,
                        poll_drain,
                        ..
                    } => targets.extend([register, request_close, poll_drain]),
                    _ => {}
                }
                budget.spend(targets.len())?;
                for target in targets {
                    if mask
                        .get(target.0 as usize)
                        .is_none_or(|stored| *stored != mask[index])
                    {
                        return Err(unsupported(
                            "mixed stored/ordinary callback edge",
                            Some(i.span),
                        ));
                    }
                }
            }
        }
    }
    Ok(Some(mask))
}
