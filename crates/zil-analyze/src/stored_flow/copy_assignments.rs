//! The original assignment identifies publication, while the actual SSA
//! graph proves that preparation creates no escaping source of authority.
use super::*;

pub(crate) fn scalar_assignment_value(ty: &Type) -> bool {
    ty.is_integer() || ty.is_float() || matches!(ty, Type::Bool | Type::Unit)
}

pub(super) fn reservations(
    module: &TypedModule,
    f: &TypedFunction,
    b: &mut Budget,
) -> CompileResult<Vec<bool>> {
    b.lookup(
        b.semantic_copy_writes.len(),
        "original Copy assignment index",
    )?;
    if b.semantic_copy_writes
        .range((f.id, ValueId(0))..=(f.id, ValueId(u32::MAX)))
        .next()
        .is_none()
    {
        return Ok(Vec::new());
    }

    fee(b, f.values.len().saturating_mul(4))?;
    let mut producers = vec![None; f.values.len()];
    let mut uses = vec![0u8; f.values.len()];
    let mut reserved = vec![false; f.values.len()];
    for block in &f.blocks {
        for (at, instruction) in block.instructions.iter().enumerate() {
            fee(b, 1)?;
            if let Some(value) = instruction.result {
                let slot = producers
                    .get_mut(value.0 as usize)
                    .ok_or_else(|| failure("Copy assignment producer has no actual value"))?;
                if slot.replace((block.id, at)).is_some() {
                    return Err(failure("Copy assignment repeats an actual SSA definition"));
                }
            }
            for value in resources::uses(&instruction.kind) {
                fee(b, 1)?;
                let slot = uses
                    .get_mut(value.0 as usize)
                    .ok_or_else(|| failure("Copy assignment use has no actual value"))?;
                *slot = slot.saturating_add(1);
            }
        }
        fee(b, 1)?;
        if let Terminator::Return(Some(value))
        | Terminator::Branch {
            condition: value, ..
        } = block.terminator
        {
            let slot = uses
                .get_mut(value.0 as usize)
                .ok_or_else(|| failure("Copy assignment terminator uses an absent value"))?;
            *slot = slot.saturating_add(1);
        }
    }

    for block in &f.blocks {
        for (write, instruction) in block.instructions.iter().enumerate() {
            fee(b, 1)?;
            let InstructionKind::WriteAddress { address, .. } = instruction.kind else {
                continue;
            };
            b.lookup(
                b.semantic_copy_writes.len(),
                "original Copy assignment write",
            )?;
            if !b.semantic_copy_writes.contains(&(f.id, address)) {
                continue;
            }
            let mut chain = Vec::new();
            let mut value = address;
            let mut consumer = write;
            let start = loop {
                fee(b, 1)?;
                if uses[value.0 as usize] != 1
                    || !matches!(value_type(f, value)?, Type::Address { mutable: true, .. })
                {
                    break None;
                }
                let Some((actual, at)) = producers[value.0 as usize] else {
                    break None;
                };
                if actual != block.id || at >= consumer {
                    break None;
                }
                if chain.len() == chain.capacity() {
                    fee(b, chain.len().saturating_add(1))?;
                }
                chain.push(value);
                match block.instructions[at].kind {
                    InstructionKind::Load(_) => break Some(at),
                    InstructionKind::AddressField { address, .. }
                    | InstructionKind::AddressIndex { address, .. }
                    | InstructionKind::AddressSlice { address, .. } => {
                        value = address;
                        consumer = at;
                    }
                    _ => break None,
                }
            };
            let Some(start) = start else { continue };
            // No intervening operation may change storage, retire a root,
            // move an owner, or suspend the activation. The normal window
            // proof still checks every read and the eventual exclusive write.
            let interval = &block.instructions[start + 1..write];
            fee(b, interval.len())?;
            let mut stable = true;
            for operation in interval {
                if !preserves_destination(module, f, operation, b)? {
                    stable = false;
                    break;
                }
            }
            if stable {
                for value in chain {
                    fee(b, 1)?;
                    reserved[value.0 as usize] = true;
                }
            }
        }
    }
    Ok(reserved)
}

fn preserves_destination(
    module: &TypedModule,
    f: &TypedFunction,
    operation: &Instruction,
    b: &mut Budget,
) -> CompileResult<bool> {
    // A synchronous call with only scalar ports cannot reach storage in
    // this activation through its inputs or result. The ordinary transfer
    // still checks the actual call and each operand's stored permissions.
    match &operation.kind {
        InstructionKind::CallIndirect { callee, .. } => {
            let Type::FunctionPointer { parameters, result } = value_type(f, *callee)? else {
                return Ok(false);
            };
            fee(b, parameters.len())?;
            if !scalar_assignment_value(result) || !parameters.iter().all(scalar_assignment_value) {
                return Ok(false);
            }
            crate::class_flow::thin::types(module, f, operation, b)?;
            return Ok(true);
        }
        InstructionKind::Call {
            function,
            arguments,
        } => {
            let callee = module
                .functions
                .get(function.0 as usize)
                .filter(|entry| entry.id == *function)
                .ok_or_else(|| failure("assignment call has no actual target function"))?;
            fee(b, arguments.len())?;
            return Ok(callee.foreign.is_none()
                && callee.environment.is_none()
                && callee.call_kind == CallKind::Direct
                && callee.protocol_contracts.is_empty()
                && callee.callable_contract.is_none()
                && callee.callable_return_contract.is_none()
                && scalar_assignment_value(&callee.return_type)
                && arguments.len() == callee.parameters.len()
                && callee
                    .parameters
                    .iter()
                    .zip(arguments)
                    .all(|(parameter, argument)| {
                        let formal = callee.locals.get(parameter.0 as usize).map(|row| &row.ty);
                        formal.is_some_and(scalar_assignment_value)
                            && formal == f.values.get(argument.0 as usize)
                    }));
        }
        _ => {}
    }
    Ok(matches!(
        operation.kind,
        InstructionKind::Constant(_)
            | InstructionKind::Load(_)
            | InstructionKind::Unary { .. }
            | InstructionKind::Binary { .. }
            | InstructionKind::ConvertInteger { .. }
            | InstructionKind::ConvertNumber { .. }
            | InstructionKind::ExtractField { .. }
            | InstructionKind::ExtractIndex { .. }
            | InstructionKind::AddressField { .. }
            | InstructionKind::AddressIndex { .. }
            | InstructionKind::AddressSlice { .. }
            | InstructionKind::ReborrowAddress { .. }
            | InstructionKind::ReadAddress { .. }
            | InstructionKind::ReadDesc { .. }
            | InstructionKind::SliceLength { .. }
            | InstructionKind::ReadPackageField { .. }
            | InstructionKind::DescribeAddress { .. }
            | InstructionKind::DescribeLocal { .. }
            | InstructionKind::DescribePackageField { .. }
    ))
}

pub(super) fn deferred(instruction: &Instruction, reservations: &[bool]) -> bool {
    instruction
        .result
        .and_then(|value| reservations.get(value.0 as usize))
        .copied()
        .unwrap_or(false)
}
