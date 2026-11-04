//! Reconstruct the closed scalar transfer lane from actual functions, not names.
use super::*;

fn reject(span: zil_base::Span, message: &str) -> Vec<Diagnostic> {
    vec![Diagnostic::error(
        Phase::Analyze,
        "E-THREAD-WORKER",
        Some(span),
        message,
    )]
}
fn scalar(ty: &Type) -> bool {
    matches!(
        ty,
        Type::Unit | Type::Bool | Type::U8 | Type::U16 | Type::I32 | Type::I64
    )
}
fn shared_i32(ty: &Type) -> bool {
    matches!(ty, Type::Address { mutable: false, inner } if **inner == Type::I32 || matches!(inner.as_ref(), Type::SyncCell(_)))
}

fn worker(
    module: &TypedModule,
    root: FunctionId,
    budget: &mut Budget,
    span: zil_base::Span,
    shared: bool,
) -> CompileResult<()> {
    budget.spend(2, "thread worker traversal allocation")?;
    let mut pending = vec![root];
    let mut seen = BTreeSet::new();
    while let Some(id) = pending.pop() {
        budget.lookup(seen.len(), "thread worker visited set")?;
        if seen.contains(&id) {
            continue;
        }
        budget.spend(1, "thread worker insertion")?;
        budget.lookup(seen.len(), "thread worker ordered-set insertion")?;
        seen.insert(id);
        let function = module
            .functions
            .get(id.0 as usize)
            .ok_or_else(|| reject(span, "thread worker function is absent"))?;
        budget.spend(
            function
                .parameters
                .len()
                .saturating_add(function.values.len())
                .saturating_add(function.locals.len())
                .saturating_add(module.resources.len())
                .saturating_add(1),
            "thread worker real type and cleanup dependency inventory",
        )?;
        if function.id != id
            || function.foreign.is_some()
            || function.environment.is_some()
            || function.callable_contract.is_some()
            || function.callable_return_contract.is_some()
            || function.result_mode != PassingMode::Infer
            || !scalar(&function.return_type)
            || function
                .locals
                .iter()
                .any(|local| !scalar(&local.ty) && !(shared && shared_i32(&local.ty)))
            || function
                .values
                .iter()
                .any(|ty| !scalar(ty) && !(shared && shared_i32(ty)))
            || module
                .resources
                .iter()
                .any(|resource| resource.teardown == Some(id))
        {
            return Err(reject(
                function.span,
                "thread worker call graph requires plain scalar definitions without address, foreign, environment or cleanup dependencies",
            ));
        }
        if id == root
            && (function.parameters.len() != 1
                || function
                    .locals
                    .get(function.parameters[0].0 as usize)
                    .map(|local| &local.ty)
                    .is_none_or(|ty| {
                        if shared {
                            !shared_i32(ty)
                        } else {
                            *ty != Type::I32
                        }
                    })
                || function.return_type != Type::I32)
        {
            return Err(reject(
                span,
                "thread worker signature must be exactly fn(i32)->i32",
            ));
        }
        for block in &function.blocks {
            budget.spend(1, "thread worker block")?;
            if shared
                && let Terminator::Return(Some(value)) = block.terminator
                && function.values.get(value.0 as usize) != Some(&function.return_type)
            {
                return Err(reject(
                    function.span,
                    "thread worker return must have its actual scalar result type",
                ));
            }
            for instruction in &block.instructions {
                budget.spend(1, "thread worker positive instruction whitelist")?;
                if shared {
                    if instruction
                        .result
                        .and_then(|id| function.values.get(id.0 as usize))
                        .is_some_and(shared_i32)
                    {
                        let valid = match instruction.kind {
                            InstructionKind::Load(local) | InstructionKind::TakeLocal { local } => {
                                function
                                    .locals
                                    .get(local.0 as usize)
                                    .is_some_and(|local| shared_i32(&local.ty))
                            }
                            InstructionKind::BorrowLocal { .. }
                            | InstructionKind::ReborrowAddress { mutable: false, .. } => true,
                            _ => false,
                        };
                        if !valid {
                            return Err(reject(
                                instruction.span,
                                "shared worker address has no permitted actual producer",
                            ));
                        }
                    }
                    if let InstructionKind::Store { local, value } = instruction.kind
                        && (function
                            .values
                            .get(value.0 as usize)
                            .is_some_and(shared_i32)
                            || function
                                .locals
                                .get(local.0 as usize)
                                .is_some_and(|local| shared_i32(&local.ty)))
                        && function.locals.get(local.0 as usize).map(|local| &local.ty)
                            != function.values.get(value.0 as usize)
                    {
                        return Err(reject(
                            instruction.span,
                            "shared worker local transport changes its address type",
                        ));
                    }
                }
                match &instruction.kind {
                    InstructionKind::SyncI32 { .. } if shared => {
                        sync_instruction(module, function, instruction, budget)?
                    }
                    InstructionKind::ReadAddress { address } if shared => {
                        if !matches!(function.values.get(address.0 as usize), Some(Type::Address { mutable: false, inner }) if **inner == Type::I32)
                            || instruction
                                .result
                                .and_then(|id| function.values.get(id.0 as usize))
                                != Some(&Type::I32)
                        {
                            return Err(reject(
                                instruction.span,
                                "thread read must use an actual shared i32 address",
                            ));
                        }
                    }
                    InstructionKind::ReborrowAddress {
                        address,
                        mutable: false,
                    } if shared => {
                        if !function
                            .values
                            .get(address.0 as usize)
                            .is_some_and(shared_i32)
                            || instruction
                                .result
                                .and_then(|id| function.values.get(id.0 as usize))
                                != function.values.get(address.0 as usize)
                        {
                            return Err(reject(
                                instruction.span,
                                "thread reborrow cannot upgrade or change the shared pointee",
                            ));
                        }
                    }
                    InstructionKind::BorrowLocal { local } if shared => {
                        if function.locals.get(local.0 as usize).map(|local| &local.ty)
                            != Some(&Type::I32)
                            || !matches!(instruction.result.and_then(|id| function.values.get(id.0 as usize)), Some(Type::Address { mutable: false, inner }) if **inner == Type::I32)
                        {
                            return Err(reject(
                                instruction.span,
                                "thread local borrow requires actual scalar storage",
                            ));
                        }
                    }
                    InstructionKind::Call {
                        function: target,
                        arguments,
                    } => {
                        if shared {
                            let callee =
                                module.functions.get(target.0 as usize).ok_or_else(|| {
                                    reject(instruction.span, "thread static callee is absent")
                                })?;
                            budget.spend(
                                arguments
                                    .len()
                                    .saturating_add(callee.parameters.len())
                                    .saturating_add(1),
                                "thread actual shared call signature",
                            )?;
                            if arguments.len() != callee.parameters.len()
                                || instruction
                                    .result
                                    .and_then(|id| function.values.get(id.0 as usize))
                                    != Some(&callee.return_type)
                            {
                                return Err(reject(
                                    instruction.span,
                                    "thread call differs from the actual static callee signature",
                                ));
                            }
                            for (argument, parameter) in arguments.iter().zip(&callee.parameters) {
                                let expected = callee
                                    .locals
                                    .get(parameter.0 as usize)
                                    .map(|local| &local.ty);
                                if let Some(ty) = expected {
                                    budget.type_work(
                                        ty,
                                        false,
                                        "thread actual shared parameter type",
                                    )?;
                                }
                                if function.values.get(argument.0 as usize) != expected {
                                    return Err(reject(
                                        instruction.span,
                                        "thread call changes the actual shared address qualification",
                                    ));
                                }
                            }
                        }
                        budget.spend(1, "thread static call dependency")?;
                        pending.push(*target);
                    }
                    InstructionKind::Constant(
                        Constant::Unit
                        | Constant::Bool(_)
                        | Constant::U8(_)
                        | Constant::U16(_)
                        | Constant::I32(_)
                        | Constant::I64(_),
                    )
                    | InstructionKind::ConvertInteger { .. }
                    | InstructionKind::ConvertNumber { .. }
                    | InstructionKind::Load(_)
                    | InstructionKind::TakeLocal { .. }
                    | InstructionKind::StorageLive(_)
                    | InstructionKind::StorageDead(_)
                    | InstructionKind::ScopeEnter { .. }
                    | InstructionKind::ScopeExit { .. }
                    | InstructionKind::Store { .. }
                    | InstructionKind::Unary {
                        op: UnaryOp::Negate | UnaryOp::Not | UnaryOp::BitNot,
                        ..
                    }
                    | InstructionKind::Binary { .. } => {}
                    _ => {
                        return Err(reject(
                            instruction.span,
                            "instruction is outside the closed pure scalar thread worker profile",
                        ));
                    }
                }
            }
        }
    }
    Ok(())
}

pub(super) fn check(module: &TypedModule, budget: &mut Budget) -> CompileResult<()> {
    if !budget.scoped_threads {
        return Ok(());
    }
    if module.target.triple != "x86_64-unknown-linux-gnu" || !module.target.is_supported_x64_llvm()
    {
        return Err(vec![Diagnostic::unsupported(
            Phase::Analyze,
            "thread_scope requires the supported Linux x64 pthread ABI",
            None,
        )]);
    }
    for caller in &module.functions {
        for block in &caller.blocks {
            budget.spend(1, "thread operation block inventory")?;
            for instruction in &block.instructions {
                budget.spend(1, "thread operation instruction inventory")?;
                if matches!(
                    instruction.kind,
                    InstructionKind::WithSyncCell { .. } | InstructionKind::SyncI32 { .. }
                ) {
                    sync_instruction(module, caller, instruction, budget)?;
                    continue;
                }
                let InstructionKind::WithScopedThread {
                    worker: entry,
                    argument,
                    body,
                } = instruction.kind
                else {
                    continue;
                };
                if caller.callable_contract.is_some() {
                    return Err(vec![Diagnostic::unsupported(
                        Phase::Analyze,
                        "thread_scope in callable parameter/helper entries is not implemented",
                        Some(instruction.span),
                    )]);
                }
                if let Some(id) = caller.environment {
                    budget.spend(2, "thread parent concrete entry lookup")?;
                    let definition = module.closures.get(id.0 as usize).ok_or_else(|| {
                        reject(instruction.span, "thread parent closure identity is absent")
                    })?;
                    budget.spend(
                        definition.parameters.len().saturating_add(1),
                        "thread parent parameter inventory",
                    )?;
                    budget.type_work(
                        &definition.result,
                        false,
                        "thread parent result comparison",
                    )?;
                    if definition.id != id
                        || definition.function != caller.id
                        || caller.foreign.is_some()
                        || caller.result_mode != PassingMode::Infer
                        || definition.parameters.len() != caller.parameters.len()
                        || definition.result != caller.return_type
                        || !matches!(
                            definition.kind,
                            CallableKind::Fn | CallableKind::FnMut | CallableKind::FnOnce
                        )
                    {
                        return Err(reject(
                            instruction.span,
                            "thread parent metadata differs from its actual concrete entry",
                        ));
                    }
                    for (parameter, expected) in
                        caller.parameters.iter().zip(&definition.parameters)
                    {
                        budget.type_work(
                            expected,
                            false,
                            "thread parent actual parameter comparison",
                        )?;
                        let local = caller.locals.get(parameter.0 as usize).ok_or_else(|| {
                            reject(instruction.span, "thread parent parameter local is absent")
                        })?;
                        if local.id != *parameter || &local.ty != expected {
                            return Err(reject(
                                instruction.span,
                                "thread parent parameter differs from its actual entry",
                            ));
                        }
                    }
                }
                let payload = caller.values.get(argument.0 as usize);
                let shared = payload.is_some_and(shared_i32);
                if (payload != Some(&Type::I32) && !shared)
                    || instruction
                        .result
                        .and_then(|value| caller.values.get(value.0 as usize))
                        != Some(&Type::Tuple(vec![Type::Bool, Type::I32, Type::I32]))
                {
                    return Err(reject(
                        instruction.span,
                        "thread payload or result differs from the actual operation signature",
                    ));
                }
                let Some(Type::Closure(id)) = caller.values.get(body.0 as usize) else {
                    return Err(reject(
                        instruction.span,
                        "thread body must be an actual concrete closure",
                    ));
                };
                let definition = module
                    .closures
                    .get(id.0 as usize)
                    .ok_or_else(|| reject(instruction.span, "thread body entry is absent"))?;
                budget.spend(1, "thread body actual entry")?;
                let entry_function = module
                    .functions
                    .get(definition.function.0 as usize)
                    .ok_or_else(|| reject(instruction.span, "thread body function is absent"))?;
                if entry_function.environment != Some(*id)
                    || entry_function.foreign.is_some()
                    || !entry_function.parameters.is_empty()
                    || entry_function.return_type != Type::I32
                    || entry_function.result_mode != PassingMode::Infer
                {
                    return Err(reject(
                        instruction.span,
                        "thread body metadata differs from its actual concrete entry",
                    ));
                }
                if !definition.parameters.is_empty()
                    || definition.result != Type::I32
                    || !definition.lending_captures.is_empty()
                    || !matches!(
                        definition.kind,
                        CallableKind::Fn | CallableKind::FnMut | CallableKind::FnOnce
                    )
                {
                    return Err(reject(
                        instruction.span,
                        "thread body requires one consuming ()->i32 invocation",
                    ));
                }
                if shared {
                    budget.lookup(
                        budget.shared_thread_functions.len(),
                        "shared thread caller insertion",
                    )?;
                    budget.spend(1, "shared thread caller insertion")?;
                    budget.shared_thread_functions.insert(caller.id);
                }
                let actual_worker = module
                    .functions
                    .get(entry.0 as usize)
                    .ok_or_else(|| reject(instruction.span, "thread actual worker is absent"))?;
                budget.spend(
                    actual_worker.parameters.len().saturating_add(1),
                    "thread exact payload kind comparison",
                )?;
                if actual_worker.parameters.len() != 1
                    || actual_worker
                        .parameters
                        .first()
                        .and_then(|id| actual_worker.locals.get(id.0 as usize))
                        .map(|local| &local.ty)
                        != payload
                {
                    return Err(reject(
                        instruction.span,
                        "thread payload differs from exact worker parameter type",
                    ));
                }
                worker(module, entry, budget, instruction.span, shared)?;
            }
        }
    }
    Ok(())
}

fn sync_instruction(
    module: &TypedModule,
    caller: &TypedFunction,
    instruction: &Instruction,
    budget: &mut Budget,
) -> CompileResult<()> {
    use zil_ir::{AtomicOrdering as O, SyncCellKind as K, SyncI32Op as P};
    budget.spend(5, "sealed cell actual signature")?;
    let ty = |value: ValueId| {
        caller
            .values
            .get(value.0 as usize)
            .ok_or_else(|| reject(instruction.span, "sealed cell operand is absent"))
    };
    match instruction.kind {
        InstructionKind::SyncI32 {
            op,
            address,
            value,
            ordering,
        } => {
            let Type::Address {
                mutable: false,
                inner,
            } = ty(address)?
            else {
                return Err(reject(
                    instruction.span,
                    "cell operation requires actual shared sealed address",
                ));
            };
            let Type::SyncCell(kind) = inner.as_ref() else {
                return Err(reject(
                    instruction.span,
                    "ordinary payload cannot grant cell operation permission",
                ));
            };
            if value.is_none() != (op == P::Load)
                || value
                    .is_some_and(|value| caller.values.get(value.0 as usize) != Some(&Type::I32))
                || (*kind == K::MutexI32 && ordering != O::SeqCst)
                || (op == P::Load && matches!(ordering, O::Release | O::AcqRel))
                || (op == P::Store && matches!(ordering, O::Acquire | O::AcqRel))
            {
                return Err(reject(
                    instruction.span,
                    "cell value operand or ordering differs from actual operation",
                ));
            }
            let expected = if op == P::Store {
                Type::Unit
            } else {
                Type::I32
            };
            if instruction
                .result
                .and_then(|value| caller.values.get(value.0 as usize))
                != Some(&expected)
            {
                return Err(reject(
                    instruction.span,
                    "cell operation result has incorrect actual type",
                ));
            }
        }
        InstructionKind::WithSyncCell {
            kind,
            initial,
            body,
        } => {
            if ty(initial)? != &Type::I32 {
                return Err(reject(
                    instruction.span,
                    "cell initializer must be an actual i32",
                ));
            }
            let Type::Closure(id) = ty(body)? else {
                return Err(reject(
                    instruction.span,
                    "cell body requires an actual concrete closure",
                ));
            };
            let definition = module
                .closures
                .get(id.0 as usize)
                .ok_or_else(|| reject(instruction.span, "cell body closure is absent"))?;
            budget.spend(
                definition
                    .parameters
                    .len()
                    .saturating_add(definition.lending_captures.len())
                    .saturating_add(4),
                "cell actual body signature and cleanup",
            )?;
            let expected = Type::Address {
                mutable: false,
                inner: Box::new(Type::SyncCell(kind)),
            };
            let entry = module
                .functions
                .get(definition.function.0 as usize)
                .ok_or_else(|| reject(instruction.span, "cell body actual entry is absent"))?;
            if definition.id != *id
                || definition.parameters.as_slice() != [expected.clone()]
                || definition.result != Type::I32
                || !definition.lending_captures.is_empty()
                || !matches!(
                    definition.kind,
                    CallableKind::Fn | CallableKind::FnMut | CallableKind::FnOnce
                )
                || entry.environment != Some(*id)
                || entry.foreign.is_some()
                || entry.parameters.len() != 1
                || entry.return_type != Type::I32
                || entry.result_mode != PassingMode::Infer
                || entry
                    .parameters
                    .first()
                    .and_then(|id| entry.locals.get(id.0 as usize))
                    .map(|local| &local.ty)
                    != Some(&expected)
                || instruction
                    .result
                    .and_then(|id| caller.values.get(id.0 as usize))
                    != Some(&Type::Tuple(vec![Type::Bool, Type::I32]))
            {
                return Err(reject(
                    instruction.span,
                    "cell scope differs from actual closure entry or cleanup boundary",
                ));
            }
        }
        _ => return Err(reject(instruction.span, "not a sealed cell operation")),
    }
    Ok(())
}
