//! Independent admission for the fixed Direct scalar thread ABI.
//! There is no source annotation or producer Transfer claim to trust.
use super::*;

fn invalid(span: Option<Span>, message: &str) -> Diagnostic {
    reject("E-THREAD-WORKER", span, message)
}
fn scalar(ty: &Type) -> bool {
    matches!(
        ty,
        Type::Unit | Type::Bool | Type::U8 | Type::U16 | Type::I32 | Type::I64
    )
}
fn cas_pair(t: &Type) -> bool {
    matches!(t,Type::Tuple(fields) if fields.as_slice()==[Type::I32,Type::Bool])
}
fn shared_i32(ty: &Type) -> bool {
    matches!(ty, Type::Address { mutable: false, inner } if **inner == Type::I32 || matches!(inner.as_ref(), Type::SyncCell(_)))
}
fn mutex_head(ty: &Type) -> Option<&Type> {
    if let Type::Desc(inner) = ty {
        if let Type::Mutex { payload } = inner.as_ref() {
            return Some(payload);
        }
    }
    None
}
fn transferable(ty: &Type, budget: &mut Budget) -> Check<bool> {
    let mut stack = vec![ty];
    while let Some(current) = stack.pop() {
        budget.spend(1)?;
        match current {
            Type::Unit
            | Type::Bool
            | Type::U8
            | Type::U16
            | Type::I32
            | Type::I64
            | Type::U64
            | Type::Usize
            | Type::F32
            | Type::F64 => {}
            Type::Managed(inner) if inner.as_ref() == &Type::ByteBuffer => {}
            Type::Tuple(fields) => stack.extend(fields),
            Type::Array { element, .. } => stack.push(element),
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                stack.extend(fields.iter().map(|(_, t)| t))
            }
            Type::Enum { variants, .. } => {
                stack.extend(variants.iter().flat_map(|(_, fields)| fields))
            }
            Type::ErrorUnion { error, result } => stack.extend([error.as_ref(), result.as_ref()]),
            _ => return Ok(false),
        }
    }
    Ok(true)
}
fn worker_type(ty: &Type, payload: &Type, budget: &mut Budget) -> Check<bool> {
    budget.spend(1)?;
    if mutex_head(ty) == Some(payload)
        || matches!(ty, Type::MutexGuard { payload: p, .. } if p.as_ref() == payload)
    {
        return Ok(true);
    }
    if let Type::Address { inner, .. } = ty {
        if inner.as_ref() == payload
            || matches!(inner.as_ref(), Type::MutexGuard { payload: p, .. } if p.as_ref() == payload)
        {
            return Ok(true);
        }
        return transferable(inner, budget);
    }
    transferable(ty, budget)
}
fn mutex_worker_graph(
    module: &TypedModule,
    root: FunctionId,
    payload: &Type,
    budget: &mut Budget,
    span: Option<Span>,
) -> Check<()> {
    if !transferable(payload, budget)? {
        return Err(invalid(
            span,
            "mutex payload lacks complete Transfer cleanup closure",
        ));
    }
    budget.spend(module.functions.len())?;
    let mut visited = vec![false; module.functions.len()];
    let mut queue = std::collections::VecDeque::from([root]);
    while let Some(id) = queue.pop_front() {
        let f = module
            .functions
            .get(id.0 as usize)
            .ok_or_else(|| invalid(span, "mutex worker callee absent"))?;
        if std::mem::replace(&mut visited[id.0 as usize], true) {
            continue;
        }
        budget.spend(
            f.locals
                .len()
                .saturating_add(f.values.len())
                .saturating_add(module.resources.len()),
        )?;
        if f.foreign.is_some()
            || f.environment.is_some()
            || f.callable_contract.is_some()
            || f.callable_return_contract.is_some()
            || f.result_mode != PassingMode::Infer
            || f.return_type != Type::I32
            || module.resources.iter().any(|r| r.teardown == Some(id))
        {
            return Err(invalid(
                Some(f.span),
                "mutex worker has foreign, environment or cleanup dependency",
            ));
        }
        for local in &f.locals {
            if !worker_type(&local.ty, payload, budget)? {
                return Err(invalid(Some(f.span), "mutex worker local lacks Transfer"));
            }
        }
        for ty in &f.values {
            if !worker_type(ty, payload, budget)? {
                return Err(invalid(Some(f.span), "mutex worker value lacks Transfer"));
            }
        }
        if id == root
            && (f.parameters.len() != 1
                || f.locals
                    .get(f.parameters[0].0 as usize)
                    .is_none_or(|l| mutex_head(&l.ty) != Some(payload)))
        {
            return Err(invalid(
                span,
                "mutex worker requires one actual shared mutex descriptor",
            ));
        }
        for block in &f.blocks {
            for instruction in &block.instructions {
                budget.spend(1)?;
                match &instruction.kind {
                    InstructionKind::Call {
                        function,
                        arguments,
                    } => {
                        let callee =
                            module.functions.get(function.0 as usize).ok_or_else(|| {
                                invalid(Some(instruction.span), "mutex worker callee absent")
                            })?;
                        budget.spend(arguments.len().saturating_add(callee.parameters.len()))?;
                        if arguments.len() != callee.parameters.len()
                            || arguments.iter().zip(&callee.parameters).any(|(v, l)| {
                                f.values.get(v.0 as usize)
                                    != callee.locals.get(l.0 as usize).map(|local| &local.ty)
                            })
                        {
                            return Err(invalid(
                                Some(instruction.span),
                                "mutex worker call changes actual parameter qualification",
                            ));
                        }
                        queue.push_back(*function);
                    }
                    InstructionKind::MutexLock { .. }
                    | InstructionKind::MutexValue { .. }
                    | InstructionKind::MutexUnlock { .. }
                    | InstructionKind::DropMutexGuard { .. }
                    | InstructionKind::ReadAddress { .. }
                    | InstructionKind::WriteAddress { .. }
                    | InstructionKind::AddressField { .. }
                    | InstructionKind::BorrowPlaceAddress { .. }
                    | InstructionKind::ReborrowAddress { .. }
                    | InstructionKind::BorrowLocal { .. }
                    | InstructionKind::BorrowLocalMut { .. }
                    | InstructionKind::DescribeAddress { .. }
                    | InstructionKind::Load(_)
                    | InstructionKind::TakeLocal { .. }
                    | InstructionKind::Store { .. }
                    | InstructionKind::StorageLive(_)
                    | InstructionKind::StorageDead(_)
                    | InstructionKind::ScopeEnter { .. }
                    | InstructionKind::ScopeExit { .. }
                    | InstructionKind::Constant(_)
                    | InstructionKind::Unary { .. }
                    | InstructionKind::Binary { .. }
                    | InstructionKind::ConvertInteger { .. }
                    | InstructionKind::ConvertNumber { .. }
                    | InstructionKind::ExtractField { .. }
                    | InstructionKind::MakeAggregate { .. } => {}
                    _ => {
                        return Err(invalid(
                            Some(instruction.span),
                            "mutex worker instruction exceeds closed synchronous graph",
                        ));
                    }
                }
            }
        }
    }
    Ok(())
}

fn worker_graph(
    module: &TypedModule,
    root: FunctionId,
    budget: &mut Budget,
    span: Option<Span>,
    shared: bool,
) -> Check<()> {
    let entry = module
        .functions
        .get(root.0 as usize)
        .ok_or_else(|| invalid(span, "thread worker index is absent"))?;
    budget.spend(entry.parameters.len().saturating_add(1))?;
    if entry.parameters.len() != 1
        || entry.return_type != Type::I32
        || if shared {
            !shared_i32(local_type(entry, entry.parameters[0], span)?)
        } else {
            local_type(entry, entry.parameters[0], span)? != &Type::I32
        }
    {
        return Err(invalid(span, "thread worker requires exact fn(i32)->i32"));
    }
    budget.spend(module.functions.len().saturating_add(1))?;
    let mut visited = vec![false; module.functions.len()];
    let mut queue = std::collections::VecDeque::from([root]);
    while let Some(id) = queue.pop_front() {
        let index = id.0 as usize;
        let function = module
            .functions
            .get(index)
            .ok_or_else(|| invalid(span, "thread worker dependency is absent"))?;
        budget.spend(1)?;
        if std::mem::replace(&mut visited[index], true) {
            continue;
        }
        budget.spend(
            function
                .locals
                .len()
                .saturating_add(function.values.len())
                .saturating_add(module.resources.len())
                .saturating_add(1),
        )?;
        if function.id != id
            || function.foreign.is_some()
            || function.environment.is_some()
            || function.callable_contract.is_some()
            || function.callable_return_contract.is_some()
            || function.result_mode != PassingMode::Infer
            || !(scalar(&function.return_type) || (shared && cas_pair(&function.return_type)))
            || function.locals.iter().any(|local| {
                !scalar(&local.ty) && !(shared && (shared_i32(&local.ty) || cas_pair(&local.ty)))
            })
            || function
                .values
                .iter()
                .any(|ty| !scalar(ty) && !(shared && (shared_i32(ty) || cas_pair(ty))))
            || module
                .resources
                .iter()
                .any(|resource| resource.teardown == Some(id))
        {
            return Err(invalid(
                Some(function.span),
                "thread worker dependency has non-scalar storage, environment, foreign or cleanup authority",
            ));
        }
        for block in &function.blocks {
            budget.spend(1)?;
            for instruction in &block.instructions {
                budget.spend(1)?;
                match &instruction.kind {
                    InstructionKind::SyncI32 { .. }
                    | InstructionKind::SyncI32CompareExchange { .. }
                        if shared =>
                    {
                        sync_instruction(module, function, instruction, budget)?;
                    }
                    InstructionKind::ExtractField { .. }
                    | InstructionKind::MakeAggregate { .. }
                        if shared => {}
                    InstructionKind::ReadAddress { address } if shared => {
                        if !matches!(value_type(function, *address, span)?, Type::Address { mutable: false, inner } if **inner == Type::I32)
                            || instruction
                                .result
                                .map(|id| value_type(function, id, span))
                                .transpose()?
                                != Some(&Type::I32)
                        {
                            return Err(invalid(
                                span,
                                "thread read requires the actual shared i32 pointer",
                            ));
                        }
                    }
                    InstructionKind::ReborrowAddress {
                        address,
                        mutable: false,
                    } if shared => {
                        if !shared_i32(value_type(function, *address, span)?)
                            || instruction
                                .result
                                .map(|id| value_type(function, id, span))
                                .transpose()?
                                != Some(value_type(function, *address, span)?)
                        {
                            return Err(invalid(
                                span,
                                "thread reborrow must preserve shared i32 permission",
                            ));
                        }
                    }
                    InstructionKind::BorrowLocal { local } if shared => {
                        if local_type(function, *local, span)? != &Type::I32
                            || !matches!(instruction.result.map(|id| value_type(function, id, span)).transpose()?, Some(Type::Address { mutable: false, inner }) if **inner == Type::I32)
                        {
                            return Err(invalid(
                                span,
                                "thread local borrow requires actual i32 storage",
                            ));
                        }
                    }
                    InstructionKind::Call { function, .. } => {
                        budget.spend(1)?;
                        queue.push_back(*function);
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
                        return Err(invalid(
                            Some(instruction.span),
                            "thread worker instruction lacks a pure scalar transfer rule",
                        ));
                    }
                }
            }
        }
    }
    Ok(())
}

pub(super) fn instruction(
    module: &TypedModule,
    caller: &TypedFunction,
    instruction: &Instruction,
    budget: &mut Budget,
) -> Check<Option<Type>> {
    let InstructionKind::WithScopedThread {
        worker,
        argument,
        body,
    } = instruction.kind
    else {
        unreachable!()
    };
    let span = Some(instruction.span);
    budget.spend(4)?;
    if module.target.triple != "x86_64-unknown-linux-gnu" || !module.target.is_supported_x64_llvm()
    {
        return Err(unsupported(
            "thread_scope requires the supported Linux x64 pthread ABI",
            span,
        ));
    }
    if caller.callable_contract.is_some() {
        return Err(unsupported(
            "thread_scope in callable parameter/helper entries is not implemented",
            span,
        ));
    }
    if let Some(id) = caller.environment {
        budget.spend(2)?;
        let definition = closures::definition(module, id, span)?;
        budget.spend(definition.parameters.len().saturating_add(1))?;
        resources::charge_payload(&definition.result, budget)?;
        if definition.id != id
            || definition.function != caller.id
            || caller.foreign.is_some()
            || caller.result_mode != PassingMode::Infer
            || caller.parameters.len() != definition.parameters.len()
            || caller.return_type != definition.result
            || !matches!(
                definition.kind,
                CallableKind::Fn | CallableKind::FnMut | CallableKind::FnOnce
            )
        {
            return Err(invalid(
                span,
                "thread parent actual entry does not match its concrete closure definition",
            ));
        }
        for (local, expected) in caller.parameters.iter().zip(&definition.parameters) {
            resources::charge_payload(expected, budget)?;
            let actual = caller
                .locals
                .get(local.0 as usize)
                .ok_or_else(|| invalid(span, "thread parent parameter local is absent"))?;
            if actual.id != *local || local_type(caller, *local, span)? != expected {
                return Err(invalid(
                    span,
                    "thread parent actual parameter differs from its closure definition",
                ));
            }
        }
    }
    let payload = value_type(caller, argument, span)?;
    let shared = shared_i32(payload);
    let generic_mutex = mutex_head(payload);
    if payload != &Type::I32 && !shared && generic_mutex.is_none() {
        return Err(invalid(
            span,
            "thread argument is not the actual i32 payload",
        ));
    }
    if shared {
        if budget.shared_thread_functions.is_none() {
            budget.spend(budget.function_count.saturating_add(1))?;
            budget.shared_thread_functions = Some(vec![false; budget.function_count]);
        }
        budget.spend(1)?;
        budget.shared_thread_functions.as_mut().unwrap()[caller.id.0 as usize] = true;
    }
    let Type::Closure(body_id) = value_type(caller, body, span)? else {
        return Err(invalid(
            span,
            "thread body must be a concrete closure value",
        ));
    };
    let body = closures::definition(module, *body_id, span)?;
    budget.spend(1)?;
    let entry = module
        .functions
        .get(body.function.0 as usize)
        .ok_or_else(|| invalid(span, "thread body actual entry is absent"))?;
    if entry.environment != Some(*body_id)
        || entry.foreign.is_some()
        || !entry.parameters.is_empty()
        || entry.return_type != Type::I32
        || entry.result_mode != PassingMode::Infer
    {
        return Err(invalid(
            span,
            "thread body metadata differs from its actual entry",
        ));
    }
    budget.spend(
        body.parameters
            .len()
            .saturating_add(body.lending_captures.len())
            .saturating_add(1),
    )?;
    if !body.parameters.is_empty()
        || body.result != Type::I32
        || !body.lending_captures.is_empty()
        || !closures::supports(body.kind, CallableKind::FnOnce)
    {
        return Err(invalid(
            span,
            "thread body must admit one consuming zero-argument i32 call",
        ));
    }
    let actual_worker = module
        .functions
        .get(worker.0 as usize)
        .ok_or_else(|| invalid(span, "thread actual worker is absent"))?;
    budget.spend(actual_worker.parameters.len().saturating_add(1))?;
    if actual_worker.parameters.len() != 1
        || local_type(actual_worker, actual_worker.parameters[0], span)? != payload
    {
        return Err(invalid(
            span,
            "thread payload differs from exact worker parameter type",
        ));
    }
    if let Some(payload) = generic_mutex {
        mutex_worker_graph(module, worker, payload, budget, span)?;
    } else {
        worker_graph(module, worker, budget, span, shared)?;
    }
    Ok(Some(Type::Tuple(vec![Type::Bool, Type::I32, Type::I32])))
}

/// Reconstruct sealed cell operations without trusting source constructor names.
pub(super) fn sync_instruction(
    module: &TypedModule,
    caller: &TypedFunction,
    instruction: &Instruction,
    budget: &mut Budget,
) -> Check<Option<Type>> {
    let span = Some(instruction.span);
    budget.spend(4)?;
    if module.target.triple != "x86_64-unknown-linux-gnu" || !module.target.is_supported_x64_llvm()
    {
        return Err(unsupported("sealed cell runtime requires Linux x64", span));
    }
    match instruction.kind {
        InstructionKind::SyncI32CompareExchange {
            address,
            expected,
            desired,
            success,
            failure,
        } => {
            budget.spend(8)?;
            if !matches!(value_type(caller,address,span)?,Type::Address{mutable:false,inner} if inner.as_ref()==&Type::SyncCell(zil_ir::SyncCellKind::AtomicI32))
                || value_type(caller, expected, span)? != &Type::I32
                || value_type(caller, desired, span)? != &Type::I32
            {
                return Err(invalid(
                    span,
                    "compare exchange requires the actual AtomicI32 address and i32 operands",
                ));
            }
            use zil_ir::AtomicOrdering as O;
            let admitted = match success {
                O::Relaxed | O::Release => failure == O::Relaxed,
                O::Acquire | O::AcqRel => matches!(failure, O::Relaxed | O::Acquire),
                O::SeqCst => matches!(failure, O::Relaxed | O::Acquire | O::SeqCst),
            };
            if !admitted {
                return Err(invalid(
                    span,
                    "compare exchange order pair is outside the chosen C11 matrix",
                ));
            }
            Ok(Some(Type::Tuple(vec![Type::I32, Type::Bool])))
        }
        InstructionKind::SyncI32 {
            op,
            address,
            value,
            ordering,
        } => {
            let Type::Address {
                mutable: false,
                inner,
            } = value_type(caller, address, span)?
            else {
                return Err(invalid(
                    span,
                    "cell operation requires actual shared sealed address",
                ));
            };
            let Type::SyncCell(kind) = inner.as_ref() else {
                return Err(invalid(
                    span,
                    "ordinary payload is not a sealed synchronization cell",
                ));
            };
            if (op == zil_ir::SyncI32Op::Load) != value.is_none() {
                return Err(invalid(
                    span,
                    "cell operation has wrong value operand presence",
                ));
            }
            if let Some(value) = value
                && value_type(caller, value, span)? != &Type::I32
            {
                return Err(invalid(span, "cell write requires an actual i32 value"));
            }
            use zil_ir::{AtomicOrdering as O, SyncCellKind as K, SyncI32Op as P};
            if (*kind == K::MutexI32 && ordering != O::SeqCst)
                || (op == P::Load && matches!(ordering, O::Release | O::AcqRel))
                || (op == P::Store && matches!(ordering, O::Acquire | O::AcqRel))
            {
                return Err(invalid(
                    span,
                    "invalid memory ordering for actual cell operation",
                ));
            }
            Ok(Some(if op == P::Store {
                Type::Unit
            } else {
                Type::I32
            }))
        }
        InstructionKind::WithSyncCell {
            kind,
            initial,
            body,
        } => {
            if value_type(caller, initial, span)? != &Type::I32 {
                return Err(invalid(span, "cell scope initializer is not i32"));
            }
            let Type::Closure(id) = value_type(caller, body, span)? else {
                return Err(invalid(
                    span,
                    "cell scope body is not an actual concrete closure",
                ));
            };
            let definition = closures::definition(module, *id, span)?;
            budget.spend(
                definition
                    .parameters
                    .len()
                    .saturating_add(definition.lending_captures.len())
                    .saturating_add(4),
            )?;
            let parameter = Type::Address {
                mutable: false,
                inner: Box::new(Type::SyncCell(kind)),
            };
            let entry = module
                .functions
                .get(definition.function.0 as usize)
                .ok_or_else(|| invalid(span, "cell scope actual body entry is absent"))?;
            if definition.parameters.as_slice() != [parameter.clone()]
                || definition.result != Type::I32
                || !definition.lending_captures.is_empty()
                || !closures::supports(definition.kind, CallableKind::FnOnce)
                || entry.environment != Some(*id)
                || entry.foreign.is_some()
                || entry.parameters.len() != 1
                || entry.return_type != Type::I32
                || entry.result_mode != PassingMode::Infer
                || local_type(entry, entry.parameters[0], span)? != &parameter
            {
                return Err(invalid(
                    span,
                    "cell scope actual closure signature or cleanup boundary differs",
                ));
            }
            Ok(Some(Type::Tuple(vec![Type::Bool, Type::I32])))
        }
        _ => Err(invalid(span, "instruction is not a sealed cell operation")),
    }
}
