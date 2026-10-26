//! Producer-side validation of owned opaque callable results. No body facts
//! from this module are exported as authority: Verify reconstructs its own.
use super::*;

pub(super) fn check_function(
    module: &TypedModule,
    function: &TypedFunction,
    reachable: &[bool],
    budget: &mut Budget,
) -> CompileResult<()> {
    if function.callable_return_contract.is_none()
        && !matches!(function.return_type, Type::Closure(_))
    {
        return Ok(());
    }
    budget.spend(1, "callable return envelope")?;
    let contract = function
        .callable_return_contract
        .as_ref()
        .ok_or_else(|| bad("closure result has no public callable return contract"))?;
    let definition = module
        .closures
        .get(contract.closure.0 as usize)
        .ok_or_else(|| bad("callable result refers to an absent closure"))?;
    budget.spend(function.parameters.len(), "factory parameter envelope")?;
    if function.environment.is_some()
        || function.foreign.is_some()
        || function.callable_contract.is_some()
        || function.result_mode != PassingMode::Infer
        || function.return_type != Type::Closure(contract.closure)
        || definition.id != contract.closure
        || definition.kind != contract.kind
        || contract.kind == CallableKind::FunctionPointer
        || !contract.hold.is_empty()
        || !contract.call.is_empty()
        || !contract.writes.is_empty()
        || function.parameters.iter().any(|p| {
            let ty = &function.locals[p.0 as usize].ty;
            !(ty.is_copy_value()
                || matches!(ty, Type::Managed(payload) if **payload == Type::ByteBuffer || payload.is_copy_value())
                || matches!(ty, Type::Package { .. }))
        })
    {
        return Err(bad(
            "callable return contract requires an ordinary self-contained factory with its exact kind",
        ));
    }
    budget.spend(definition.captures.len(), "factory cleanup reconstruction")?;
    let mut cleanup = Vec::new();
    for (index, ty) in definition.captures.iter().enumerate() {
        if matches!(ty, Type::Managed(payload) if **payload == Type::ByteBuffer || payload.is_copy_value())
            || matches!(ty, Type::Package { .. })
        {
            cleanup.push(index as u32);
        } else if !ty.is_copy_value() {
            return Err(bad(
                "callable result captures an external address or description",
            ));
        }
    }
    budget.spend(
        cleanup.len().saturating_add(contract.cleanup.len()),
        "factory cleanup comparison",
    )?;
    if contract.cleanup != cleanup {
        return Err(bad(
            "callable return cleanup differs from owned capture indices",
        ));
    }
    let mut creations = 0usize;
    let mut returns = 0usize;
    for (block, live) in function.blocks.iter().zip(reachable) {
        budget.spend(1, "factory block")?;
        if !live {
            continue;
        }
        for instruction in &block.instructions {
            budget.spend(1, "factory value origin")?;
            if let InstructionKind::Call {
                function: callee, ..
            } = instruction.kind
            {
                let target = module
                    .functions
                    .get(callee.0 as usize)
                    .ok_or_else(|| bad("factory calls an absent function"))?;
                if target.callable_return_contract.is_some() {
                    return Err(bad(
                        "recursive or forwarded callable factories are not implemented",
                    ));
                }
            }
            let result_type = instruction
                .result
                .map(|v| {
                    function
                        .values
                        .get(v.0 as usize)
                        .ok_or_else(|| bad("factory instruction result outside value table"))
                })
                .transpose()?;
            if result_type == Some(&function.return_type) {
                match instruction.kind {
                    InstructionKind::MakeClosure { closure, .. } if closure == contract.closure => {
                        creations += 1
                    }
                    InstructionKind::Load(_) | InstructionKind::TakeLocal { .. } => {}
                    _ => {
                        return Err(bad(
                            "factory result is not derived from its concrete environment",
                        ));
                    }
                }
            }
        }
        if let Terminator::Return(value) = block.terminator {
            let value = value.ok_or_else(|| bad("callable factory returns no environment"))?;
            if function.values.get(value.0 as usize) != Some(&function.return_type) {
                return Err(bad(
                    "callable factory return differs from the hidden closure identity",
                ));
            }
            returns += 1;
        }
    }
    if creations != 1 || returns == 0 {
        return Err(bad(
            "callable factory requires one environment creation site and a returned identity",
        ));
    }
    Ok(())
}
