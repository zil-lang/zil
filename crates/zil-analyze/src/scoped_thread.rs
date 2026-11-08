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
        Type::Unit
            | Type::Bool
            | Type::U8
            | Type::U16
            | Type::I32
            | Type::I64
            | Type::U64
            | Type::Usize
    )
}
fn cas_pair(t: &Type) -> bool {
    matches!(t,Type::Tuple(fields) if matches!(fields.as_slice(), [Type::I32 | Type::I64 | Type::U64 | Type::Usize, Type::Bool]))
}
fn shared_i32(ty: &Type) -> bool {
    matches!(ty, Type::Address { mutable: false, inner } if **inner == Type::I32 || matches!(inner.as_ref(), Type::SyncCell(_)))
}
fn wide_integer(kind: zil_ir::SyncCellKind) -> Option<Type> {
    use zil_ir::SyncCellKind as K;
    match kind {
        K::AtomicI64 => Some(Type::I64),
        K::AtomicU64 => Some(Type::U64),
        K::AtomicUsize => Some(Type::Usize),
        K::MutexI32 | K::AtomicI32 => None,
    }
}
fn mutex_head(ty: &Type) -> Option<&Type> {
    match ty {
        Type::Desc(inner) => match inner.as_ref() {
            Type::Mutex { payload } => Some(payload),
            _ => None,
        },
        _ => None,
    }
}
fn transferable(ty: &Type, budget: &mut Budget) -> CompileResult<bool> {
    let mut pending = vec![ty];
    while let Some(ty) = pending.pop() {
        budget.spend(1, "mutex thread complete Transfer payload")?;
        match ty {
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
            Type::Managed(inner) if matches!(inner.as_ref(), Type::ByteBuffer) => {}
            Type::Tuple(fields) => pending.extend(fields),
            Type::Array { element, .. } => pending.push(element),
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                pending.extend(fields.iter().map(|(_, ty)| ty))
            }
            Type::Enum { variants, .. } => {
                pending.extend(variants.iter().flat_map(|(_, fields)| fields))
            }
            Type::ErrorUnion { error, result } => pending.extend([error.as_ref(), result.as_ref()]),
            _ => return Ok(false),
        }
    }
    Ok(true)
}
fn mutex_worker_type(ty: &Type, payload: &Type, budget: &mut Budget) -> CompileResult<bool> {
    budget.spend(1, "mutex worker actual type")?;
    if mutex_head(ty) == Some(payload)
        || matches!(ty, Type::MutexGuard { payload: p, .. } if p.as_ref() == payload)
    {
        return Ok(true);
    }
    if let Type::Address { inner, .. } = ty {
        if matches!(inner.as_ref(), Type::MutexGuard { payload: p, .. } if p.as_ref() == payload)
            || inner.as_ref() == payload
        {
            return Ok(true);
        }
        return transferable(inner, budget);
    }
    transferable(ty, budget)
}
fn mutex_worker(
    module: &TypedModule,
    root: FunctionId,
    payload: &Type,
    budget: &mut Budget,
    span: zil_base::Span,
) -> CompileResult<()> {
    if !transferable(payload, budget)? {
        return Err(reject(
            span,
            "mutex payload lacks complete thread Transfer and cleanup closure",
        ));
    }
    let mut pending = vec![root];
    let mut seen = BTreeSet::new();
    while let Some(id) = pending.pop() {
        budget.lookup(seen.len(), "mutex worker call graph")?;
        if !seen.insert(id) {
            continue;
        }
        let f = module
            .functions
            .get(id.0 as usize)
            .ok_or_else(|| reject(span, "mutex worker callee absent"))?;
        budget.spend(
            f.locals
                .len()
                .saturating_add(f.values.len())
                .saturating_add(module.resources.len()),
            "mutex worker actual inventory",
        )?;
        if f.call_kind != CallKind::Direct
            || f.foreign.is_some()
            || f.environment.is_some()
            || f.callable_contract.is_some()
            || f.callable_return_contract.is_some()
            || f.result_mode != PassingMode::Infer
            || !(scalar(&f.return_type) || cas_pair(&f.return_type))
            || f.locals.iter().try_fold(false, |bad, l| {
                Ok::<_, Vec<Diagnostic>>(bad || !mutex_worker_type(&l.ty, payload, budget)?)
            })?
            || f.values.iter().try_fold(false, |bad, ty| {
                Ok::<_, Vec<Diagnostic>>(bad || !mutex_worker_type(ty, payload, budget)?)
            })?
            || module.resources.iter().any(|r| r.teardown == Some(id))
        {
            return Err(reject(
                f.span,
                "mutex worker graph has nontransferable type or foreign cleanup dependency",
            ));
        }
        if id == root
            && (f.return_type != Type::I32
                || f.parameters.len() != 1
                || f.locals
                    .get(f.parameters[0].0 as usize)
                    .is_none_or(|l| mutex_head(&l.ty) != Some(payload)))
        {
            return Err(reject(
                span,
                "mutex worker root requires one actual shared mutex descriptor and i32 result",
            ));
        }
        for block in &f.blocks {
            for i in &block.instructions {
                budget.spend(1, "mutex worker instruction")?;
                match &i.kind {
                    InstructionKind::Call {
                        function,
                        arguments,
                    } => {
                        let callee = module
                            .functions
                            .get(function.0 as usize)
                            .ok_or_else(|| reject(i.span, "mutex worker callee absent"))?;
                        budget.spend(
                            arguments.len().saturating_add(callee.parameters.len()),
                            "mutex worker actual call",
                        )?;
                        if arguments.len() != callee.parameters.len()
                            || arguments.iter().zip(&callee.parameters).any(|(v, l)| {
                                f.values.get(v.0 as usize)
                                    != callee.locals.get(l.0 as usize).map(|local| &local.ty)
                            })
                        {
                            return Err(reject(
                                i.span,
                                "mutex worker call changes actual parameter qualification",
                            ));
                        }
                        pending.push(*function);
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
                        return Err(reject(
                            i.span,
                            "mutex worker instruction exceeds closed synchronous graph",
                        ));
                    }
                }
            }
        }
    }
    Ok(())
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
                    InstructionKind::SyncI32 { .. }
                    | InstructionKind::SyncI32CompareExchange { .. }
                    | InstructionKind::SyncInteger { .. }
                    | InstructionKind::SyncIntegerCompareExchange { .. }
                        if shared =>
                    {
                        sync_instruction(module, function, instruction, budget)?
                    }
                    InstructionKind::ExtractField { .. }
                    | InstructionKind::MakeAggregate { .. }
                        if shared => {}
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
                        | Constant::I64(_)
                        | Constant::U64(_)
                        | Constant::Usize(_),
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
                    InstructionKind::WithSyncCell { .. }
                        | InstructionKind::SyncI32 { .. }
                        | InstructionKind::SyncI32CompareExchange { .. }
                        | InstructionKind::SyncInteger { .. }
                        | InstructionKind::SyncIntegerCompareExchange { .. }
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
                let generic_mutex = payload.and_then(mutex_head);
                if (payload != Some(&Type::I32) && !shared && generic_mutex.is_none())
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
                if let Some(payload) = generic_mutex {
                    mutex_worker(module, entry, payload, budget, instruction.span)?;
                } else {
                    worker(module, entry, budget, instruction.span, shared)?;
                }
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
        InstructionKind::SyncI32CompareExchange {
            address,
            expected,
            desired,
            success,
            failure,
        } => {
            budget.spend(8, "actual strong CAS signature and order matrix")?;
            if !matches!(ty(address)?,Type::Address{mutable:false,inner} if inner.as_ref()==&Type::SyncCell(K::AtomicI32))
                || ty(expected)? != &Type::I32
                || ty(desired)? != &Type::I32
            {
                return Err(reject(
                    instruction.span,
                    "compare exchange requires the actual AtomicI32 address and i32 operands",
                ));
            }
            let admitted = match success {
                O::Relaxed | O::Release => failure == O::Relaxed,
                O::Acquire | O::AcqRel => matches!(failure, O::Relaxed | O::Acquire),
                O::SeqCst => matches!(failure, O::Relaxed | O::Acquire | O::SeqCst),
            };
            if !admitted {
                return Err(reject(
                    instruction.span,
                    "compare exchange order pair is outside the chosen C11 matrix",
                ));
            }
            if instruction
                .result
                .and_then(|v| caller.values.get(v.0 as usize))
                != Some(&Type::Tuple(vec![Type::I32, Type::Bool]))
            {
                return Err(reject(
                    instruction.span,
                    "compare exchange must return actual observed i32 and success bool",
                ));
            }
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
            if !matches!(kind, K::MutexI32 | K::AtomicI32)
                || value.is_none() != (op == P::Load)
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
        InstructionKind::SyncIntegerCompareExchange {
            address,
            expected,
            desired,
            success,
            failure,
        } => {
            budget.spend(
                8,
                "wide atomic strong CAS actual signature and order matrix",
            )?;
            let Type::Address {
                mutable: false,
                inner,
            } = ty(address)?
            else {
                return Err(reject(
                    instruction.span,
                    "wide atomic CAS needs shared sealed address",
                ));
            };
            let Type::SyncCell(kind) = inner.as_ref() else {
                return Err(reject(
                    instruction.span,
                    "wide atomic CAS cannot use ordinary address",
                ));
            };
            let Some(integer) = wide_integer(*kind) else {
                return Err(reject(
                    instruction.span,
                    "wide atomic CAS requires matching wide cell",
                ));
            };
            if ty(expected)? != &integer || ty(desired)? != &integer {
                return Err(reject(
                    instruction.span,
                    "wide atomic CAS operands differ from cell width",
                ));
            }
            let admitted = match success {
                O::Relaxed | O::Release => failure == O::Relaxed,
                O::Acquire | O::AcqRel => matches!(failure, O::Relaxed | O::Acquire),
                O::SeqCst => matches!(failure, O::Relaxed | O::Acquire | O::SeqCst),
            };
            if !admitted {
                return Err(reject(
                    instruction.span,
                    "wide atomic CAS order pair is invalid",
                ));
            }
            if instruction
                .result
                .and_then(|v| caller.values.get(v.0 as usize))
                != Some(&Type::Tuple(vec![integer, Type::Bool]))
            {
                return Err(reject(
                    instruction.span,
                    "wide atomic CAS result type differs from cell",
                ));
            }
        }
        InstructionKind::SyncInteger {
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
                    "wide atomic operation needs shared sealed address",
                ));
            };
            let Type::SyncCell(kind) = inner.as_ref() else {
                return Err(reject(
                    instruction.span,
                    "wide atomic operation cannot use ordinary address",
                ));
            };
            let Some(integer) = wide_integer(*kind) else {
                return Err(reject(
                    instruction.span,
                    "wide atomic operation requires wide cell",
                ));
            };
            if value.is_none() != (op == zil_ir::SyncIntegerOp::Load)
                || value.is_some_and(|v| caller.values.get(v.0 as usize) != Some(&integer))
                || (op == zil_ir::SyncIntegerOp::Load && matches!(ordering, O::Release | O::AcqRel))
                || (op == zil_ir::SyncIntegerOp::Store
                    && matches!(ordering, O::Acquire | O::AcqRel))
            {
                return Err(reject(
                    instruction.span,
                    "wide atomic value or ordering differs from operation",
                ));
            }
            let expected = if op == zil_ir::SyncIntegerOp::Store {
                Type::Unit
            } else {
                integer
            };
            if instruction
                .result
                .and_then(|v| caller.values.get(v.0 as usize))
                != Some(&expected)
            {
                return Err(reject(
                    instruction.span,
                    "wide atomic operation result has wrong width",
                ));
            }
        }
        InstructionKind::WithSyncCell {
            kind,
            initial,
            body,
        } => {
            let integer = wide_integer(kind).unwrap_or(Type::I32);
            if ty(initial)? != &integer {
                return Err(reject(
                    instruction.span,
                    "cell initializer differs from actual sealed width",
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
