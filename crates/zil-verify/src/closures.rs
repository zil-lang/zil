//! Independent closure proof boundary. Frontend/analyzer claims never
//! substitute for checked signatures, executable operands, or reconstructed Hold.
use super::*;

pub(super) fn definition(
    module: &TypedModule,
    id: ClosureId,
    span: Option<Span>,
) -> Check<&ClosureDefinition> {
    module.closures.get(id.0 as usize).ok_or_else(|| {
        reject(
            "E-VERIFY-CLOSURE",
            span,
            "closure identity is outside the definition table",
        )
    })
}

pub(super) fn check_definitions(
    module: &TypedModule,
    limits: &Limits,
    budget: &mut Budget,
) -> Check<()> {
    if module.closures.len() > limits.max_functions
        || module.closures.len() > module.functions.len()
    {
        return Err(limit("closure count exceeds the function bound"));
    }
    check_layouts(module, limits, budget)?;
    budget.spend(module.functions.len())?;
    let mut entries = vec![None; module.functions.len()];
    for (index, closure) in module.closures.iter().enumerate() {
        budget.spend(1)?;
        if closure.id.0 as usize != index {
            return Err(reject(
                "E-VERIFY-CLOSURE",
                None,
                "ClosureId must equal its vector index",
            ));
        }
        if !matches!(
            closure.kind,
            CallableKind::Fn | CallableKind::FnMut | CallableKind::FnOnce
        ) {
            return Err(unsupported(
                "this slice verifies concrete Fn, FnMut and FnOnce environments",
                None,
            ));
        }
        if closure.captures.len() > limits.max_locals
            || closure.parameters.len() > limits.max_locals
        {
            return Err(limit(
                "closure field or parameter count exceeds the selected bound",
            ));
        }
        budget.spend(closure.captures.len())?;
        budget.spend(closure.parameters.len().saturating_add(1))?;
        // Capture fields include independently checked finite owning Packages.
        // Hold, view binding and authority are reconstructed from the raw CFG.
        for ty in &closure.captures {
            if ty.is_shared_package_address() {
                return Err(unsupported(
                    "Store element address capture requires a separate retained element-window contract",
                    None,
                ));
            }
            if ty.is_shared_resource_head_address() {
                return Err(unsupported(
                    "resource-head address capture requires a separate retained head-window contract",
                    None,
                ));
            }
            if dynamic_store::is_carrier(ty) {
                return Err(unsupported(
                    "Store capture requires a separate retained storage profile",
                    None,
                ));
            }
            if packages::has_array_fields(ty) {
                return Err(unsupported(
                    "Package capture with owning array fields requires a retained nested-field protocol",
                    None,
                ));
            }
            if resources::arrays::is_owned_type(ty) {
                return Err(unsupported(
                    "owned array capture requires a separate retained array protocol",
                    None,
                ));
            }
            if errors::owned(ty) {
                return Err(unsupported(
                    "owning error-union captures are outside the finite closure profile",
                    None,
                ));
            }
            if let Type::Closure(id) = ty {
                self_contained(module, *id, budget)?;
            }
            if byte_buffers::is_desc(ty) {
                return Err(unsupported(
                    "borrowed byte storage captures are outside the finite profile",
                    None,
                ));
            }
            if slices::is_window(ty) {
                return Err(unsupported(
                    "borrowed slice captures are outside the finite slice profile",
                    None,
                ));
            }
            check_local_type(ty, None, module, budget)?;
        }
        if closure.kind == CallableKind::Fn
            && closure
                .captures
                .iter()
                .any(|t| matches!(t, Type::Address { mutable: true, .. }))
        {
            return Err(reject(
                "E-VERIFY-CLOSURE-CALL",
                None,
                "exclusive capture requires an exclusive environment entry",
            ));
        }
        for ty in closure
            .parameters
            .iter()
            .chain(std::iter::once(&closure.result))
        {
            check_type(ty, None)?;
        }
        let function = module
            .functions
            .get(closure.function.0 as usize)
            .ok_or_else(|| {
                reject(
                    "E-VERIFY-CLOSURE-ENTRY",
                    None,
                    "closure entry function is absent",
                )
            })?;
        let entry = &mut entries[closure.function.0 as usize];
        if entry.replace(closure.id).is_some() || function.environment != Some(closure.id) {
            return Err(reject(
                "E-VERIFY-CLOSURE-ENTRY",
                Some(function.span),
                "closure definition and entry environment must be one-to-one",
            ));
        }
        if function.parameters.len() != closure.parameters.len()
            || function.return_type != closure.result
        {
            return Err(reject(
                "E-VERIFY-CLOSURE-ENTRY",
                Some(function.span),
                "closure entry signature differs from its definition",
            ));
        }
        for (local, ty) in function.parameters.iter().zip(&closure.parameters) {
            if local_type(function, *local, Some(function.span))? != ty {
                return Err(reject(
                    "E-VERIFY-CLOSURE-ENTRY",
                    Some(function.span),
                    "closure entry parameter differs from its definition",
                ));
            }
        }
    }
    for (index, function) in module.functions.iter().enumerate() {
        budget.spend(1)?;
        if function.environment != entries[index] {
            return Err(reject(
                "E-VERIFY-CLOSURE-ENTRY",
                Some(function.span),
                "function has an orphaned or mismatched environment identity",
            ));
        }
    }
    Ok(())
}

/// Inline fields must form a finite layout before any backend sizing or
/// alignment recursion. A raw ClosureId is not evidence of acyclicity.
fn check_layouts(module: &TypedModule, limits: &Limits, budget: &mut Budget) -> Check<()> {
    budget.spend(module.closures.len().saturating_mul(3))?;
    let mut state = vec![0u8; module.closures.len()];
    let mut heights = vec![0usize; module.closures.len()];
    let mut expanded = vec![0usize; module.closures.len()];
    for root in 0..state.len() {
        let mut stack = vec![(ClosureId(root as u32), false, 0usize)];
        while let Some((id, exit, depth)) = stack.pop() {
            budget.spend(1)?;
            let definition = definition(module, id, None)?;
            if depth > limits.max_call_depth {
                return Err(limit("closure layout depth exceeds selected bound"));
            }
            if exit {
                budget.spend(definition.captures.len())?;
                let height = definition
                    .captures
                    .iter()
                    .filter_map(|ty| {
                        if let Type::Closure(child) = ty {
                            Some(heights[child.0 as usize] + 1)
                        } else {
                            None
                        }
                    })
                    .max()
                    .unwrap_or(0);
                if height > limits.max_call_depth {
                    return Err(limit("closure layout depth exceeds selected bound"));
                }
                let mut work = 1usize;
                for ty in &definition.captures {
                    let field = if let Type::Closure(child) = ty {
                        expanded[child.0 as usize]
                    } else {
                        type_layout_work(ty, budget)?
                    };
                    work = work
                        .checked_add(field)
                        .ok_or_else(|| limit("closure layout expansion overflow"))?;
                }
                budget
                    .spend(work)
                    .map_err(|_| limit("closure layout expansion budget exceeded"))?;
                expanded[id.0 as usize] = work;
                heights[id.0 as usize] = height;
                state[id.0 as usize] = 2;
                continue;
            }
            match state[id.0 as usize] {
                2 => continue,
                1 => {
                    return Err(reject(
                        "E-VERIFY-CLOSURE-LAYOUT",
                        None,
                        "recursive inline closure environment layout",
                    ));
                }
                _ => {}
            }
            state[id.0 as usize] = 1;
            stack.push((id, true, depth));
            budget.spend(definition.captures.len())?;
            for ty in &definition.captures {
                if let Type::Closure(child) = ty {
                    stack.push((*child, false, depth + 1));
                }
            }
        }
    }
    // Each local/value instance can require another machine layout/cleanup
    // traversal. Charge expanded work, including array multiplicity, rather
    // than granting a large DAG a cheap ClosureId at every use site.
    let instances = module
        .functions
        .iter()
        .try_fold(0usize, |count, function| {
            count
                .checked_add(function.locals.len())
                .and_then(|count| count.checked_add(function.values.len()))
                .and_then(|count| count.checked_add(1))
        })
        .ok_or_else(|| limit("closure instance table size overflow"))?;
    budget.spend(instances)?;
    let mut types = module
        .functions
        .iter()
        .flat_map(|f| {
            f.locals
                .iter()
                .map(|local| &local.ty)
                .chain(&f.values)
                .chain(std::iter::once(&f.return_type))
        })
        .map(|ty| (ty, 1usize))
        .collect::<Vec<_>>();
    while let Some((ty, count)) = types.pop() {
        budget.spend(1)?;
        match ty {
            Type::Closure(id) => {
                definition(module, *id, None)?;
                let work = count
                    .checked_mul(expanded[id.0 as usize])
                    .ok_or_else(|| limit("closure instance expansion overflow"))?;
                budget
                    .spend(work)
                    .map_err(|_| limit("closure instance expansion budget exceeded"))?;
            }
            Type::Array { element, length } => {
                let count = count
                    .checked_mul(*length as usize)
                    .ok_or_else(|| limit("closure array expansion overflow"))?;
                types.push((element, count));
            }
            // Other containers do not admit Closure elements in this slice;
            // their own type checks retain the precise rejection boundary.
            _ => {}
        }
    }
    Ok(())
}

/// Size the actual bounded type tree used by inline layout predicates, type
/// rendering and alignment. Pointer shapes still carry a checked pointee tree;
/// only explicit Closure fields are expanded through the separate DAG above.
/// Array repetition is charged by the owning-array caller, not by Copy arrays.
pub(crate) fn type_layout_work(ty: &Type, budget: &mut Budget) -> Check<usize> {
    let mut pending = vec![ty];
    let mut work = 0usize;
    while let Some(ty) = pending.pop() {
        budget.spend(1)?;
        work = work
            .checked_add(1)
            .ok_or_else(|| limit("inline type layout work overflow"))?;
        match ty {
            Type::Array { element, .. }
            | Type::Slice { element }
            | Type::OwnedStore { element }
            | Type::Address { inner: element, .. }
            | Type::Managed(element)
            | Type::Desc(element)
            | Type::Opaque {
                hidden: element, ..
            } => pending.push(element),
            Type::Tuple(fields) => {
                budget.spend(fields.len())?;
                pending.extend(fields);
            }
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                budget.spend(fields.len())?;
                pending.extend(fields.iter().map(|(_, ty)| ty));
            }
            Type::Enum { variants, .. } => {
                budget.spend(variants.len())?;
                for (_, fields) in variants {
                    budget.spend(fields.len())?;
                    pending.extend(fields);
                }
            }
            Type::ErrorUnion { error, result } => {
                budget.spend(2)?;
                pending.extend([error.as_ref(), result.as_ref()]);
            }
            Type::FunctionPointer { parameters, result }
            | Type::ForeignCallback { parameters, result } => {
                budget.spend(parameters.len().saturating_add(1))?;
                pending.extend(parameters);
                pending.push(result);
            }
            _ => {}
        }
    }
    Ok(work)
}

pub(super) fn self_contained(
    module: &TypedModule,
    root: ClosureId,
    budget: &mut Budget,
) -> Check<()> {
    let mut visited = std::collections::BTreeSet::new();
    let mut pending = vec![root];
    while let Some(id) = pending.pop() {
        budget.spend(1)?;
        if !visited.insert(id) {
            continue;
        }
        let definition = definition(module, id, None)?;
        budget.spend(definition.captures.len())?;
        for ty in &definition.captures {
            match ty {
                Type::Closure(child) => pending.push(*child),
                Type::Managed(_) | Type::Package { .. } => {}
                ty if ty.is_copy_value() => {}
                _ => {
                    return Err(unsupported(
                        "owned nested closure capture requires a self-contained environment",
                        None,
                    ));
                }
            }
        }
    }
    Ok(())
}

/// A unique carrier is required for owned resource fields and for a consuming
/// entry. This is a representation classification, not proof of responsibility.
pub(super) fn owned_type(module: &TypedModule, ty: &Type, budget: &mut Budget) -> Check<bool> {
    let Type::Closure(id) = ty else {
        return Ok(false);
    };
    let definition = definition(module, *id, None)?;
    budget.spend(definition.captures.len().saturating_add(1))?;
    Ok(definition.kind == CallableKind::FnOnce
        || definition.captures.iter().any(|ty| {
            matches!(
                ty,
                Type::Managed(_) | Type::Package { .. } | Type::Closure(_)
            )
        }))
}

pub(super) fn check_resource_instruction(
    module: &TypedModule,
    function: &TypedFunction,
    instruction: &Instruction,
    budget: &mut Budget,
) -> Check<Option<Type>> {
    let span = Some(instruction.span);
    match &instruction.kind {
        InstructionKind::BorrowCapture { index, mutable } => {
            let environment = function.environment.ok_or_else(|| {
                reject(
                    "E-VERIFY-CAPTURE",
                    span,
                    "capture borrow requires an entry environment",
                )
            })?;
            let definition = definition(module, environment, span)?;
            let Some(ty @ Type::Closure(_)) = definition.captures.get(*index as usize) else {
                return Err(reject(
                    "E-VERIFY-CAPTURE",
                    span,
                    "capture borrow requires an actual inline closure field",
                ));
            };
            if *mutable && definition.kind == CallableKind::Fn {
                return Err(reject(
                    "E-VERIFY-CLOSURE-CALL",
                    span,
                    "exclusive capture borrow requires an exclusive or consuming entry",
                ));
            }
            Ok(Some(Type::Address {
                mutable: *mutable,
                inner: Box::new(ty.clone()),
            }))
        }
        InstructionKind::DescribeCapture { index } | InstructionKind::TakeCapture { index } => {
            let environment = function.environment.ok_or_else(|| {
                reject(
                    "E-VERIFY-CAPTURE",
                    span,
                    "resource capture operation requires an entry environment",
                )
            })?;
            let definition = definition(module, environment, span)?;
            let Some(ty) = definition.captures.get(*index as usize) else {
                return Err(reject(
                    "E-VERIFY-CAPTURE",
                    span,
                    "owned capture index is outside the environment",
                ));
            };
            if matches!(instruction.kind, InstructionKind::TakeCapture { .. }) {
                if !matches!(ty, Type::Managed(_) | Type::Closure(_)) {
                    return Err(reject(
                        "E-VERIFY-CAPTURE",
                        span,
                        "capture take requires an actual Managed field or owned Closure field",
                    ));
                }
                if definition.kind != CallableKind::FnOnce {
                    return Err(reject(
                        "E-VERIFY-CLOSURE-CALL",
                        span,
                        "moving an owned capture requires a Consume entry",
                    ));
                }
                Ok(Some(ty.clone()))
            } else {
                let Type::Managed(inner) = ty else {
                    return Err(reject(
                        "E-VERIFY-CAPTURE",
                        span,
                        "resource description requires an actual Managed field",
                    ));
                };
                Ok(Some(Type::Desc(inner.clone())))
            }
        }
        InstructionKind::DropClosure { local } => {
            let ty = local_type(function, *local, span)?;
            if !owned_type(module, ty, budget)? {
                return Err(reject(
                    "E-VERIFY-CLOSURE-CALL",
                    span,
                    "DropClosure requires an owned or consuming environment position",
                ));
            }
            Ok(None)
        }
        InstructionKind::CallClosureBorrowed {
            callee,
            kind,
            arguments,
        } => {
            let Type::Address { mutable, inner } = value_type(function, *callee, span)? else {
                return Err(reject(
                    "E-VERIFY-CLOSURE-CALL",
                    span,
                    "borrowed closure call requires an actual environment address",
                ));
            };
            let Type::Closure(id) = inner.as_ref() else {
                return Err(reject(
                    "E-VERIFY-CLOSURE-CALL",
                    span,
                    "borrowed closure call pointee must be a concrete environment",
                ));
            };
            let definition = definition(module, *id, span)?;
            if !matches!(kind, CallableKind::Fn | CallableKind::FnMut)
                || !supports(definition.kind, *kind)
                || (*kind == CallableKind::FnMut && !mutable)
            {
                return Err(reject(
                    "E-VERIFY-CLOSURE-CALL",
                    span,
                    "borrowed closure call mode lacks its concrete entry or address permission",
                ));
            }
            if arguments.len() != definition.parameters.len() {
                return Err(reject(
                    "E-VERIFY-CLOSURE-CALL",
                    span,
                    "borrowed closure call arity mismatch",
                ));
            }
            for (value, ty) in arguments.iter().zip(&definition.parameters) {
                if value_type(function, *value, span)? != ty {
                    return Err(reject(
                        "E-VERIFY-CLOSURE-CALL",
                        span,
                        "borrowed closure argument mismatch",
                    ));
                }
            }
            Ok((definition.result != Type::Unit).then(|| definition.result.clone()))
        }
        InstructionKind::CallClosureShared { arguments, .. }
        | InstructionKind::CallClosureOnce { arguments, .. } => {
            let (ty, expected_kind) = match instruction.kind {
                InstructionKind::CallClosureShared { callee, .. } => {
                    (local_type(function, callee, span)?, CallableKind::Fn)
                }
                InstructionKind::CallClosureOnce { callee, .. } => {
                    (value_type(function, callee, span)?, CallableKind::FnOnce)
                }
                _ => unreachable!(),
            };
            let Type::Closure(id) = ty else {
                return Err(reject(
                    "E-VERIFY-CLOSURE-CALL",
                    span,
                    "closure call requires a concrete environment",
                ));
            };
            let definition = definition(module, *id, span)?;
            if !supports(definition.kind, expected_kind) {
                return Err(reject(
                    "E-VERIFY-CLOSURE-CALL",
                    span,
                    "closure call mode does not match its concrete entry",
                ));
            }
            if arguments.len() != definition.parameters.len() {
                return Err(reject(
                    "E-VERIFY-CLOSURE-CALL",
                    span,
                    "closure call arity mismatch",
                ));
            }
            for (value, ty) in arguments.iter().zip(&definition.parameters) {
                if value_type(function, *value, span)? != ty {
                    return Err(reject(
                        "E-VERIFY-CLOSURE-CALL",
                        span,
                        "closure call argument mismatch",
                    ));
                }
            }
            Ok((definition.result != Type::Unit).then(|| definition.result.clone()))
        }
        _ => unreachable!(),
    }
}

/// The environment carries address dependencies even when a helper body has no
/// explicit Address-typed temporary of its own. Desc dependencies are tracked
/// separately by the resource checker; they are not address lifetime claims.
pub(super) fn holds_addresses(module: &TypedModule, ty: &Type) -> bool {
    match ty {
        Type::Address { .. } => true,
        Type::Closure(id) => module.closures[id.0 as usize]
            .captures
            .iter()
            .any(|ty| matches!(ty, Type::Address { .. })),
        _ => false,
    }
}

/// The requested mode is an invocation capability, not a change to the actual
/// entry ABI. Consuming adapters retain their explicit CallClosureOnce cleanup.
pub(super) fn supports(actual: CallableKind, requested: CallableKind) -> bool {
    matches!(
        (actual, requested),
        (
            CallableKind::Fn,
            CallableKind::Fn | CallableKind::FnMut | CallableKind::FnOnce
        ) | (
            CallableKind::FnMut,
            CallableKind::FnMut | CallableKind::FnOnce
        ) | (CallableKind::FnOnce, CallableKind::FnOnce)
    )
}
