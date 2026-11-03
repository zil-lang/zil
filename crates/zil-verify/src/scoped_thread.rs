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
fn worker_graph(
    module: &TypedModule,
    root: FunctionId,
    budget: &mut Budget,
    span: Option<Span>,
) -> Check<()> {
    let entry = module
        .functions
        .get(root.0 as usize)
        .ok_or_else(|| invalid(span, "thread worker index is absent"))?;
    budget.spend(entry.parameters.len().saturating_add(1))?;
    if entry.parameters.len() != 1
        || entry.return_type != Type::I32
        || local_type(entry, entry.parameters[0], span)? != &Type::I32
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
            || function.locals.iter().any(|local| !scalar(&local.ty))
            || function.values.iter().any(|ty| !scalar(ty))
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
                    | InstructionKind::Unary { .. }
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
    if caller.environment.is_some() || caller.callable_contract.is_some() {
        return Err(unsupported(
            "thread_scope is implemented only in ordinary Direct functions; closure and callable helper entries are not implemented",
            span,
        ));
    }
    if value_type(caller, argument, span)? != &Type::I32 {
        return Err(invalid(
            span,
            "thread argument is not the actual i32 payload",
        ));
    }
    let Type::Closure(body_id) = value_type(caller, body, span)? else {
        return Err(invalid(
            span,
            "thread body must be a concrete closure value",
        ));
    };
    let body = closures::definition(module, *body_id, span)?;
    budget.spend(1)?;
    let entry = module.functions.get(body.function.0 as usize)
        .ok_or_else(|| invalid(span, "thread body actual entry is absent"))?;
    if entry.environment != Some(*body_id) || entry.foreign.is_some()
        || !entry.parameters.is_empty() || entry.return_type != Type::I32
        || entry.result_mode != PassingMode::Infer
    { return Err(invalid(span, "thread body metadata differs from its actual entry")); }
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
    worker_graph(module, worker, budget, span)?;
    Ok(Some(Type::Tuple(vec![Type::Bool, Type::I32, Type::I32])))
}
