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

fn worker(
    module: &TypedModule,
    root: FunctionId,
    budget: &mut Budget,
    span: zil_base::Span,
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
            || function.locals.iter().any(|local| !scalar(&local.ty))
            || function.values.iter().any(|ty| !scalar(ty))
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
                    != Some(&Type::I32)
                || function.return_type != Type::I32)
        {
            return Err(reject(
                span,
                "thread worker signature must be exactly fn(i32)->i32",
            ));
        }
        for block in &function.blocks {
            budget.spend(1, "thread worker block")?;
            for instruction in &block.instructions {
                budget.spend(1, "thread worker positive instruction whitelist")?;
                match &instruction.kind {
                    InstructionKind::Call { function, .. } => {
                        budget.spend(1, "thread static call dependency")?;
                        pending.push(*function);
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
                let InstructionKind::WithScopedThread {
                    worker: entry,
                    argument,
                    body,
                } = instruction.kind
                else {
                    continue;
                };
                if caller.environment.is_some() || caller.callable_contract.is_some() {
                    return Err(reject(
                        instruction.span,
                        "thread_scope in nested or callable helper entries is not implemented",
                    ));
                }
                if caller.values.get(argument.0 as usize) != Some(&Type::I32)
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
                worker(module, entry, budget, instruction.span)?;
            }
        }
    }
    Ok(())
}
