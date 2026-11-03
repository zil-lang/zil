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
fn shared_i32(ty: &Type) -> bool {
    matches!(ty, Type::Address { mutable: false, inner } if **inner == Type::I32)
}

// Independently follow the actual producer, including local address transport.
// Type eligibility alone must not admit a Store/lending/owning projection.
fn stable_payload(
    module: &TypedModule,
    f: &TypedFunction,
    argument: ValueId,
    budget: &mut Budget,
    span: Option<Span>,
) -> Check<()> {
    budget.spend(
        f.values
            .len()
            .saturating_add(f.locals.len())
            .saturating_add(1),
    )?;
    let mut values = vec![false; f.values.len()];
    let mut locals = vec![false; f.locals.len()];
    let mut pending = vec![(false, argument.0 as usize)];
    let mut grounded = false;
    while let Some((local, index)) = pending.pop() {
        budget.spend(1)?;
        let seen = if local {
            &mut locals[index]
        } else {
            &mut values[index]
        };
        if std::mem::replace(seen, true) {
            continue;
        }
        if local {
            budget.spend(f.parameters.len())?;
            if f.parameters.iter().any(|id| id.0 as usize == index) {
                if !matches!(&f.locals[index].ty, Type::Address { inner, .. } if **inner == Type::I32)
                {
                    return Err(invalid(
                        span,
                        "thread payload parameter has no i32 address authority",
                    ));
                }
                grounded = true;
                continue;
            }
        }
        let mut produced = false;
        for block in &f.blocks {
            budget.spend(block.instructions.len().saturating_add(1))?;
            for instruction in &block.instructions {
                if local {
                    if let InstructionKind::Store { local, value } = instruction.kind
                        && local.0 as usize == index
                    {
                        budget.spend(1)?;
                        pending.push((false, value.0 as usize));
                        produced = true;
                    }
                    continue;
                }
                if instruction.result != Some(ValueId(index as u32)) {
                    continue;
                }
                produced = true;
                match instruction.kind {
                    InstructionKind::BorrowLocal { local }
                    | InstructionKind::BorrowLocalMut { local }
                        if f.locals[local.0 as usize].ty == Type::I32 =>
                    {
                        grounded = true
                    }
                    InstructionKind::Load(local) | InstructionKind::TakeLocal { local } if matches!(&f.locals[local.0 as usize].ty, Type::Address { inner, .. } if **inner == Type::I32) =>
                    {
                        budget.spend(1)?;
                        pending.push((true, local.0 as usize));
                    }
                    InstructionKind::ReborrowAddress {
                        address,
                        mutable: false,
                    } if matches!(&f.values[address.0 as usize], Type::Address { inner, .. } if **inner == Type::I32) =>
                    {
                        budget.spend(1)?;
                        pending.push((false, address.0 as usize));
                    }
                    InstructionKind::ReadCapture { index }
                    | InstructionKind::BorrowCapture {
                        index,
                        mutable: false,
                    } if f
                        .environment
                        .and_then(|id| module.closures.get(id.0 as usize))
                        .and_then(|c| c.captures.get(index as usize))
                        .is_some_and(|ty| {
                            matches!(ty, Type::Address { inner, .. } if **inner == Type::I32)
                                || *ty == Type::I32
                        }) =>
                    {
                        grounded = true
                    }
                    _ => {
                        return Err(unsupported(
                            "shared thread dynamic, lending or owning projection storage is not implemented",
                            span,
                        ));
                    }
                }
            }
        }
        if !produced {
            return Err(invalid(
                span,
                "shared thread payload has no actual storage producer",
            ));
        }
    }
    if !grounded {
        return Err(invalid(
            span,
            "shared thread payload lacks a stable storage root",
        ));
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
                    InstructionKind::ReadAddress { address } if shared => {
                        if !shared_i32(value_type(function, *address, span)?)
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
                            || !instruction
                                .result
                                .map(|id| value_type(function, id, span))
                                .transpose()?
                                .is_some_and(shared_i32)
                        {
                            return Err(invalid(
                                span,
                                "thread reborrow must preserve shared i32 permission",
                            ));
                        }
                    }
                    InstructionKind::BorrowLocal { local } if shared => {
                        if local_type(function, *local, span)? != &Type::I32
                            || !instruction
                                .result
                                .map(|id| value_type(function, id, span))
                                .transpose()?
                                .is_some_and(shared_i32)
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
    if payload != &Type::I32 && !shared {
        return Err(invalid(
            span,
            "thread argument is not the actual i32 payload",
        ));
    }
    if shared {
        stable_payload(module, caller, argument, budget, span)?;
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
    worker_graph(module, worker, budget, span, shared)?;
    Ok(Some(Type::Tuple(vec![Type::Bool, Type::I32, Type::I32])))
}
