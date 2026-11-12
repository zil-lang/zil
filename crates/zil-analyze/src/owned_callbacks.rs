//! Ordinary owned callback data is separate from environment admission. These
//! checks reconstruct actual signatures; no Types or Verify fact is consumed.
use super::*;

pub(super) fn data(module: &TypedModule, ty: &Type, budget: &mut Budget) -> CompileResult<()> {
    // Store's ordinary value profile also admits callback-bearing State. That
    // independent container permission is not an ordinary callback data slot.
    budget.type_work(ty, false, "owned callback closed data tree")?;
    fn closed(ty: &Type) -> bool {
        match ty {
            Type::Closure(_) | Type::Opaque { .. } | Type::Address { .. } | Type::Desc(_) => false,
            Type::Tuple(fields) => fields.iter().all(closed),
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                fields.iter().all(|(_, ty)| closed(ty))
            }
            Type::Enum { variants, .. } => {
                variants.iter().all(|(_, fields)| fields.iter().all(closed))
            }
            Type::ErrorUnion { error, result } => closed(error) && closed(result),
            Type::Array { element, .. }
            | Type::Slice { element }
            | Type::OwnedStore { element }
            | Type::Managed(element) => closed(element),
            // Thin function values remain atomic Copy data under their old
            // signature/FFI checks; they do not carry an environment or F.
            _ => true,
        }
    }
    if !closed(ty) {
        return Err(bad(
            "owned callback data cannot hide an environment, opaque or borrowed dependency",
        ));
    }
    match ty {
        Type::Package { .. } | Type::Enum { .. } | Type::ErrorUnion { .. } => {
            resources::sums::admit(module, ty, budget).map(|_| ())
        }
        _ => {
            budget.type_work(ty, true, "owned callback finite Copy companion")?;
            if ty.is_copy_value() {
                Ok(())
            } else {
                Err(bad(
                    "owned callback data requires ordinary Product/Enum/ErrorUnion or finite Copy",
                ))
            }
        }
    }
}

/// Inspect the actual input pointee separately from closed result data. A loan
/// remains an ordinary loan and is never part of the returned responsibility.
pub(super) fn input(module: &TypedModule, ty: &Type, budget: &mut Budget) -> CompileResult<()> {
    if let Type::Address { inner, .. } = ty {
        budget.type_work(ty, false, "borrowed input owned-result callback")?;
        match inner.as_ref() {
            Type::Slice { element } => {
                budget.type_work(element, true, "borrowed callback slice element")?;
                if !element.is_copy_value() {
                    return Err(bad("borrowed callback slice requires finite Copy elements"));
                }
                Ok(())
            }
            pointee => data(module, pointee, budget),
        }
    } else {
        data(module, ty, budget)
    }
}

pub(super) fn signature(
    module: &TypedModule,
    definition: &ClosureDefinition,
    budget: &mut Budget,
) -> CompileResult<()> {
    let entry = module
        .functions
        .get(definition.function.0 as usize)
        .ok_or_else(|| bad("owned callback entry is absent"))?;
    budget.spend(
        definition.parameters.len().saturating_add(1),
        "owned callback actual entry",
    )?;
    budget.type_work(
        &definition.result,
        false,
        "owned callback entry result comparison",
    )?;
    if !module
        .closures
        .get(definition.id.0 as usize)
        .is_some_and(|actual| std::ptr::eq(actual, definition))
        || definition.kind == CallableKind::FunctionPointer
        || !definition.lending_captures.is_empty()
        || entry.environment != Some(definition.id)
        || entry.foreign.is_some()
        || entry.callable_return_contract.is_some()
        || entry.callable_contract.is_some()
        || entry.parameters.len() != definition.parameters.len()
        || entry.return_type != definition.result
        || entry.result_mode != PassingMode::Infer
    {
        return Err(bad(
            "owned callback definition differs from its actual ordinary entry",
        ));
    }
    for (local, ty) in entry.parameters.iter().zip(&definition.parameters) {
        budget.type_work(ty, false, "owned callback entry parameter comparison")?;
        if entry.locals.get(local.0 as usize).map(|l| &l.ty) != Some(ty) {
            return Err(bad("owned callback actual parameter type differs"));
        }
    }
    budget.spend(entry.blocks.len(), "owned callback capture access blocks")?;
    for block in &entry.blocks {
        budget.spend(
            block.instructions.len(),
            "owned callback capture access instructions",
        )?;
        for instruction in &block.instructions {
            if let InstructionKind::WriteCapture { index, value } = instruction.kind {
                let ty = definition
                    .captures
                    .get(index as usize)
                    .ok_or_else(|| bad("owned callback capture write index is absent"))?;
                budget.type_work(ty, true, "owned callback capture write type")?;
                if definition.kind == CallableKind::Fn
                    || !ty.is_copy_value()
                    || entry.values.get(value.0 as usize) != Some(ty)
                    || instruction.result.is_some()
                {
                    return Err(bad(
                        "owned callback capture write requires a mutable entry and exact Copy field",
                    ));
                }
            }
        }
    }
    Ok(())
}

fn container_profile(ty: &Type, budget: &mut Budget) -> CompileResult<()> {
    budget.type_work(ty, false, "owned callback container profile exclusion")?;
    fn contained(ty: &Type, nested: bool, owned: &BTreeSet<ClosureId>) -> bool {
        match ty {
            Type::Closure(id) => nested && owned.contains(id),
            Type::Tuple(fields) => fields.iter().any(|ty| contained(ty, true, owned)),
            Type::Package { fields, .. } | Type::Record { fields, .. } => {
                fields.iter().any(|(_, ty)| contained(ty, true, owned))
            }
            Type::Enum { variants, .. } => variants
                .iter()
                .any(|(_, fields)| fields.iter().any(|ty| contained(ty, true, owned))),
            Type::ErrorUnion { error, result } => {
                contained(error, true, owned) || contained(result, true, owned)
            }
            Type::Array { element, .. }
            | Type::Slice { element }
            | Type::OwnedStore { element }
            | Type::Managed(element)
            | Type::Opaque {
                hidden: element, ..
            } => contained(element, true, owned),
            // A borrowed direct environment remains a concrete call
            // position; an address does not turn it into a container.
            Type::Address { inner, .. } | Type::Desc(inner) => contained(inner, nested, owned),
            _ => false,
        }
    }
    if contained(ty, false, &budget.owned_callbacks) {
        return Err(bad(
            "owned callback signature does not grant a State, array or container profile",
        ));
    }
    Ok(())
}

// A compiler replacement slot has no public container permission. Reconstruct
// its complete operation boundary before exempting its root type from the
// general container exclusion; a call edge to a helper grants nothing here.
fn private_slot_candidate(ty: &Type, budget: &mut Budget) -> CompileResult<bool> {
    let Some(id) = super::forward::internal_slot(ty) else {
        return Ok(false);
    };
    budget.lookup(
        budget.owned_callbacks.len(),
        "private slot owned signature lookup",
    )?;
    if budget.owned_callbacks.contains(&id) {
        return Ok(true);
    }
    budget.lookup(
        budget.forward_closures.len(),
        "private slot forward signature lookup",
    )?;
    Ok(budget.forward_closures.contains(&id))
}

fn private_slots(f: &TypedFunction, budget: &mut Budget) -> CompileResult<()> {
    budget.spend(f.parameters.len(), "private callable slot public boundary")?;
    for local in &f.parameters {
        if private_slot_candidate(&f.locals[local.0 as usize].ty, budget)? {
            return Err(bad(
                "private callable slot cannot be a public parameter or result",
            ));
        }
    }
    if private_slot_candidate(&f.return_type, budget)? {
        return Err(bad(
            "private callable slot cannot be a public parameter or result",
        ));
    }
    budget.spend(f.blocks.len(), "private callable slot blocks")?;
    for block in &f.blocks {
        for i in &block.instructions {
            budget.spend(1, "private callable slot instruction")?;
            let output = i.result.and_then(|v| f.values.get(v.0 as usize));
            let local = match &i.kind {
                InstructionKind::Load(l)
                | InstructionKind::StorageLive(l)
                | InstructionKind::StorageDead(l) => Some(*l),
                InstructionKind::TakeLocal { local }
                | InstructionKind::Store { local, .. }
                | InstructionKind::BorrowLocal { local }
                | InstructionKind::BorrowLocalMut { local }
                | InstructionKind::DescribeLocal { local }
                | InstructionKind::DropManaged { local }
                | InstructionKind::DropPackage { local }
                | InstructionKind::DropErrorUnion { local }
                | InstructionKind::DropClosure { local }
                | InstructionKind::RegisterDefer { local, .. }
                | InstructionKind::DropOwnedArray { local }
                | InstructionKind::ProjectOwnedArrayField { local, .. }
                | InstructionKind::TakeOwnedArrayElement { local, .. }
                | InstructionKind::StoreOwnedArrayElement { local, .. }
                | InstructionKind::DropOwnedArrayElement { local, .. }
                | InstructionKind::BorrowOwnedArrayElement { local, .. }
                | InstructionKind::BorrowPackageField { local, .. }
                | InstructionKind::ReadPackageField { local, .. }
                | InstructionKind::DescribePackageField { local, .. }
                | InstructionKind::TakePackageField { local, .. }
                | InstructionKind::StorePackageField { local, .. }
                | InstructionKind::DropPackageField { local, .. }
                | InstructionKind::BindPackageCapture { local, .. }
                | InstructionKind::EndPackageCapture { local, .. }
                | InstructionKind::VariantTagLocal { local, .. }
                | InstructionKind::ReadVariantLocal { local, .. }
                | InstructionKind::BorrowVariantLocal { local, .. }
                | InstructionKind::BorrowFieldLocal { local, .. }
                | InstructionKind::UnpackVariant { local, .. }
                | InstructionKind::DropEnum { local } => Some(*local),
                InstructionKind::CallClosureShared { callee, .. }
                | InstructionKind::CallClosureMut { callee, .. } => Some(*callee),
                _ => None,
            };
            if let InstructionKind::UnpackVariant { fields, .. } = &i.kind {
                budget.spend(fields.len(), "private callable slot unpack targets")?;
                for local in fields {
                    if private_slot_candidate(&f.locals[local.0 as usize].ty, budget)? {
                        return Err(bad("private callable slot cannot be an unpack target"));
                    }
                }
            }
            if let Some(local) = local
                && private_slot_candidate(&f.locals[local.0 as usize].ty, budget)?
                && let Some(id) = super::forward::internal_slot(&f.locals[local.0 as usize].ty)
            {
                let ty = &f.locals[local.0 as usize].ty;
                let element = Type::Closure(id);
                budget.type_work(ty, false, "private callable slot exact operation")?;
                let valid = match &i.kind {
                    InstructionKind::StorageLive(_)
                    | InstructionKind::StorageDead(_)
                    | InstructionKind::DropOwnedArray { .. } => i.result.is_none(),
                    InstructionKind::TakeLocal { .. } => output == Some(ty),
                    InstructionKind::Store { value, .. } => {
                        i.result.is_none() && f.values.get(value.0 as usize) == Some(ty)
                    }
                    InstructionKind::TakeOwnedArrayElement { index: 0, .. } => {
                        output == Some(&element)
                    }
                    InstructionKind::DropOwnedArrayElement { index: 0, .. } => i.result.is_none(),
                    InstructionKind::StoreOwnedArrayElement {
                        index: 0, value, ..
                    } => {
                        i.result.is_none()
                            && f.locals[local.0 as usize].mutable
                            && f.values.get(value.0 as usize) == Some(&element)
                    }
                    InstructionKind::BorrowOwnedArrayElement {
                        index: 0, mutable, ..
                    } => {
                        (!*mutable || f.locals[local.0 as usize].mutable)
                            && matches!(output, Some(Type::Address { mutable: write, inner })
                                if write == mutable && inner.as_ref() == &element)
                    }
                    _ => false,
                };
                if !valid {
                    return Err(bad(
                        "private callable slot uses an unsupported or inexact local operation",
                    ));
                }
            }
            if let Some(ty) = output
                && private_slot_candidate(ty, budget)?
            {
                let valid = match &i.kind {
                    InstructionKind::MakeOwnedArray { elements } if elements.len() == 1 => {
                        let Some(Type::Array { element, .. }) = output else {
                            unreachable!()
                        };
                        f.values.get(elements[0].0 as usize) == Some(element.as_ref())
                    }
                    InstructionKind::TakeLocal { local } => {
                        output == Some(&f.locals[local.0 as usize].ty)
                    }
                    _ => false,
                };
                if !valid {
                    return Err(bad(
                        "private callable slot has an unsupported value producer",
                    ));
                }
            }
            for value in super::resources::uses(&i.kind) {
                budget.spend(1, "private callable slot operand boundary")?;
                let ty = f
                    .values
                    .get(value.0 as usize)
                    .ok_or_else(|| bad("private slot operand is absent"))?;
                if private_slot_candidate(ty, budget)?
                    && !matches!(&i.kind, InstructionKind::Store { local, value: stored }
                    if *stored == value && &f.locals[local.0 as usize].ty == ty)
                {
                    return Err(bad(
                        "private callable slot escapes its local replacement boundary",
                    ));
                }
            }
        }
        let terminal = match block.terminator {
            Terminator::Return(value) => value,
            Terminator::Branch { condition, .. } => Some(condition),
            _ => None,
        };
        if let Some(value) = terminal {
            budget.spend(1, "private callable slot terminator boundary")?;
            if private_slot_candidate(&f.values[value.0 as usize], budget)? {
                return Err(bad("private callable slot cannot escape through return"));
            }
        }
    }
    Ok(())
}

fn local_profiles(f: &TypedFunction, budget: &mut Budget) -> CompileResult<bool> {
    let mut private = false;
    for ty in f.locals.iter().map(|local| &local.ty).chain(&f.values) {
        let slot = private_slot_candidate(ty, budget)?;
        if slot {
            // Same charged type traversal as the general container exclusion.
            budget.type_work(ty, false, "private callable slot root type")?;
            private = true;
        } else {
            container_profile(ty, budget)?;
        }
    }
    container_profile(&f.return_type, budget)?;
    if private {
        private_slots(f, budget)?;
    }
    Ok(private)
}

pub(super) fn check(module: &TypedModule, budget: &mut Budget) -> CompileResult<()> {
    if budget.owned_callbacks.is_empty() && budget.owned_callback_helpers.is_empty() {
        return Ok(());
    }
    budget.spend(
        module.closures.len(),
        "owned callback capture container profiles",
    )?;
    for definition in &module.closures {
        budget.spend(
            definition.captures.len(),
            "owned callback capture container slots",
        )?;
        for ty in &definition.captures {
            container_profile(ty, budget)?;
        }
    }
    budget.spend(
        module.functions.len(),
        "owned callback window function inventory",
    )?;
    let mut windows = vec![false; module.functions.len()];
    let mut edges = Vec::new();
    budget.spend(
        budget
            .owned_callbacks
            .len()
            .saturating_add(budget.owned_callback_helpers.len()),
        "owned callback window roots",
    )?;
    if budget.forward_functions.is_empty() && !budget.owned_callbacks.is_empty() {
        budget.spend(module.functions.len(), "owned callable strong window flags")?;
        budget.forward_functions = vec![false; module.functions.len()];
    }
    for id in &budget.owned_callbacks {
        let function = module.closures[id.0 as usize].function.0 as usize;
        windows[function] = true;
        // A real owned-signature environment's position moves invalidate its
        // loans even in an ordinary caller with no Parameter-return relation.
        budget.forward_functions[function] = true;
    }
    for id in &budget.owned_callback_helpers {
        windows[id.0 as usize] = true;
    }
    // Every environment admitted to Parameter forwarding must have its body
    // checked even when no consumer invokes it in this module. This includes
    // its actual owning children, not just the top-level return type.
    budget.spend(
        budget.forward_closures.len(),
        "forward actual body window roots",
    )?;
    for id in &budget.forward_closures {
        let function = module.closures[id.0 as usize].function.0 as usize;
        windows[function] = true;
        budget.forward_functions[function] = true;
    }
    budget.spend(module.functions.len(), "owned callback call functions")?;
    for f in &module.functions {
        // Admission is structural even for ordinary bodies without a keep
        // edge. The flag only selects additional independent window checks.
        if local_profiles(f, budget)? {
            budget.spend(1, "private callable slot actual body root")?;
            windows[f.id.0 as usize] = true;
            if budget.forward_functions.is_empty() {
                budget.spend(module.functions.len(), "private callable slot window flags")?;
                budget.forward_functions = vec![false; module.functions.len()];
            }
            budget.forward_functions[f.id.0 as usize] = true;
        }
        budget.spend(f.blocks.len(), "owned callback call blocks")?;
        for b in &f.blocks {
            budget.spend(b.instructions.len(), "owned callback call instructions")?;
            for i in &b.instructions {
                if let InstructionKind::MakeClosure { closure, captures } = &i.kind {
                    budget.lookup(
                        budget.owned_callbacks.len(),
                        "owned callback construction lookup",
                    )?;
                    let forwarding = if budget.forward_closures.is_empty() {
                        false
                    } else {
                        budget.lookup(
                            budget.forward_closures.len(),
                            "forward actual construction lookup",
                        )?;
                        budget.forward_closures.contains(closure)
                    };
                    if budget.owned_callbacks.contains(closure) || forwarding {
                        let definition = &module.closures[closure.0 as usize];
                        budget.spend(captures.len(), "owned callback actual captures")?;
                        if captures.len() != definition.captures.len()
                            || i.result.and_then(|v| f.values.get(v.0 as usize))
                                != Some(&Type::Closure(*closure))
                        {
                            return Err(bad("owned callback construction signature differs"));
                        }
                        for (value, ty) in captures.iter().zip(&definition.captures) {
                            budget.type_work(ty, false, "owned callback actual capture type")?;
                            if f.values.get(value.0 as usize) != Some(ty) {
                                return Err(bad("owned callback actual capture type differs"));
                            }
                        }
                    }
                    continue;
                }
                if let InstructionKind::Call {
                    function,
                    arguments,
                } = &i.kind
                {
                    if function.0 as usize >= windows.len() {
                        return Err(bad("owned callback call graph target is absent"));
                    }
                    budget.spend(1, "owned callback ordinary call edge")?;
                    edges.push((f.id.0 as usize, function.0 as usize));
                    budget.lookup(
                        budget.owned_callback_helpers.len(),
                        "owned helper actual signature lookup",
                    )?;
                    if budget.owned_callback_helpers.contains(function) {
                        let target = &module.functions[function.0 as usize];
                        budget.spend(arguments.len(), "owned helper actual arguments")?;
                        budget.type_work(
                            &target.return_type,
                            false,
                            "owned helper actual result",
                        )?;
                        if arguments.len() != target.parameters.len()
                            || !(i.result.and_then(|v| f.values.get(v.0 as usize))
                                == Some(&target.return_type)
                                || (i.result.is_none() && target.return_type == Type::Unit))
                        {
                            return Err(bad("owned callback helper call signature differs"));
                        }
                        for (value, local) in arguments.iter().zip(&target.parameters) {
                            let ty = &target.locals[local.0 as usize].ty;
                            budget.type_work(ty, false, "owned helper actual parameter")?;
                            if f.values.get(value.0 as usize) != Some(ty) {
                                return Err(bad(
                                    "owned callback helper actual argument type differs",
                                ));
                            }
                        }
                    }
                    continue;
                }
                let (ty, arguments) = match &i.kind {
                    InstructionKind::CallClosureShared { callee, arguments }
                    | InstructionKind::CallClosureMut { callee, arguments } => {
                        (f.locals.get(callee.0 as usize).map(|l| &l.ty), arguments)
                    }
                    InstructionKind::CallClosure { callee, arguments }
                    | InstructionKind::CallClosureOnce { callee, arguments }
                    | InstructionKind::CallClosureBorrowed {
                        callee, arguments, ..
                    } => (f.values.get(callee.0 as usize), arguments),
                    _ => continue,
                };
                let ty = match ty {
                    Some(Type::Address { inner, .. }) => inner.as_ref(),
                    Some(ty) => ty,
                    None => return Err(bad("callback callee is absent")),
                };
                let Type::Closure(id) = ty else {
                    return Err(bad("callback callee has no environment"));
                };
                let c = module
                    .closures
                    .get(id.0 as usize)
                    .ok_or_else(|| bad("callback environment is absent"))?;
                budget.spend(1, "owned callback concrete call edge")?;
                edges.push((f.id.0 as usize, c.function.0 as usize));
                budget.lookup(
                    budget.owned_callbacks.len(),
                    "owned callback signature lookup",
                )?;
                let forwarding = if budget.forward_closures.is_empty() {
                    false
                } else {
                    budget.lookup(
                        budget.forward_closures.len(),
                        "forward actual callback signature lookup",
                    )?;
                    budget.forward_closures.contains(id)
                };
                if !budget.owned_callbacks.contains(id) && !forwarding {
                    continue;
                }
                if let InstructionKind::CallClosureMut { callee, .. } = i.kind
                    && !f.locals[callee.0 as usize].mutable
                {
                    return Err(bad(
                        "owned callback FnMut requires an actual writable environment",
                    ));
                }
                budget.spend(arguments.len(), "owned callback actual arguments")?;
                budget.type_work(&c.result, false, "owned callback actual result")?;
                if arguments.len() != c.parameters.len()
                    || !(i.result.and_then(|v| f.values.get(v.0 as usize)) == Some(&c.result)
                        || (i.result.is_none() && c.result == Type::Unit))
                {
                    return Err(bad("owned callback call signature differs"));
                }
                for (value, ty) in arguments.iter().zip(&c.parameters) {
                    budget.type_work(ty, false, "owned callback actual parameter")?;
                    if f.values.get(value.0 as usize) != Some(ty) {
                        return Err(bad("owned callback actual argument type differs"));
                    }
                }
            }
        }
    }
    loop {
        budget.spend(edges.len(), "owned callback connected window functions")?;
        let mut changed = false;
        for &(caller, callee) in &edges {
            if windows[caller] || windows[callee] {
                changed |= !windows[caller] || !windows[callee];
                windows[caller] = true;
                windows[callee] = true;
            }
            if !budget.forward_functions.is_empty() {
                budget.spend(1, "forward connected address window edge")?;
                if budget.forward_functions[caller] || budget.forward_functions[callee] {
                    changed |=
                        !budget.forward_functions[caller] || !budget.forward_functions[callee];
                    budget.forward_functions[caller] = true;
                    budget.forward_functions[callee] = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
    budget.owned_callback_window_functions = windows;
    Ok(())
}
