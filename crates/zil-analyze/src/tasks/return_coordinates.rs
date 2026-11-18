//! Reconstruct the coordinate change at a direct Task creation return.
use super::*;
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn checked(
    module: &TypedModule,
    f: &TypedFunction,
    block: &BasicBlock,
    value: ValueId,
    b: &mut Budget,
) -> CompileResult<bool> {
    let Some(actual) = f.values.get(value.0 as usize) else {
        return Ok(false);
    };
    let Type::ErrorUnion { result, .. } = actual else {
        return Ok(false);
    };
    let Type::Managed(task) = result.as_ref() else {
        return Ok(false);
    };
    let Type::Task {
        result: output,
        retained_life,
    } = task.as_ref()
    else {
        return Ok(false);
    };
    b.spend(
        block.instructions.len().saturating_add(1),
        "Task creation return inventory",
    )?;
    let Some((
        at,
        Instruction {
            kind:
                InstructionKind::TaskCreate {
                    worker,
                    arguments,
                    port_bindings,
                },
            ..
        },
    )) = block
        .instructions
        .iter()
        .enumerate()
        .find(|(_, op)| op.result == Some(value))
    else {
        return Ok(false);
    };
    let worker = module
        .functions
        .get(worker.0 as usize)
        .filter(|function| function.id == *worker)
        .ok_or_else(|| bad("Task return has no actual worker declaration"))?;
    b.type_work(actual, false, "Task physical result geometry")?;
    b.type_work(&f.return_type, false, "Task public result geometry")?;
    if worker.return_type != **output
        || worker.parameters.len() != arguments.len()
        || owned::borrowed_shape(worker, b)? != *retained_life
        || retained_life.ports.len() != port_bindings.len()
    {
        return Ok(false);
    }
    let mut origins = BTreeMap::new();
    let mut inputs = BTreeSet::new();
    for (port, binding) in port_bindings.iter().enumerate() {
        let zil_ir::StorageBase::Parameter(index) = binding.source.base else {
            return Ok(false);
        };
        let Some(mut arg) = arguments.get(index as usize).copied() else {
            return Ok(false);
        };
        if binding.port as usize != port
            || !binding.source.path.is_empty()
            || retained_life.retained.get(port).is_none_or(|r| {
                r.path != [zil_ir::LifeTypeProjection::Parameter(index)]
                    || r.binder != retained_life.ports[port].binder
                    || r.class != 0
            })
        {
            return Ok(false);
        }
        let mut before = at;
        let load = loop {
            b.spend(before.saturating_add(1), "Task return actual source chain")?;
            let Some((position, op)) = block.instructions[..before]
                .iter()
                .enumerate()
                .rev()
                .find(|(_, op)| op.result == Some(arg))
            else {
                return Ok(false);
            };
            match op.kind {
                InstructionKind::Load(local) => break local,
                InstructionKind::ReborrowAddress { address, .. } => {
                    before = position;
                    arg = address;
                }
                _ => return Ok(false),
            }
        };
        for body in &f.blocks {
            b.spend(
                body.instructions.len().saturating_add(1),
                "Task factory parameter generation",
            )?;
            for op in &body.instructions {
                if matches!(
                    op.kind,
                    InstructionKind::Load(_) | InstructionKind::StorageDead(_)
                ) {
                    continue;
                }
                for local in op.kind.local_operands() {
                    b.spend(1, "Task factory parameter access")?;
                    if local == load {
                        return Ok(false);
                    }
                }
            }
        }
        b.spend(
            f.parameters
                .len()
                .saturating_add(origins.len())
                .saturating_add(1),
            "Task return source identity",
        )?;
        let Some(parameter) = f
            .parameters
            .iter()
            .position(|p| *p == load)
            .and_then(|n| u32::try_from(n).ok())
        else {
            return Ok(false);
        };
        if !inputs.insert(parameter) || origins.insert(index, parameter).is_some() {
            return Ok(false);
        }
    }
    let mut projected = actual.clone();
    let Type::ErrorUnion { result, .. } = &mut projected else {
        unreachable!()
    };
    let Type::Managed(task) = result.as_mut() else {
        unreachable!()
    };
    let Type::Task { retained_life, .. } = task.as_mut() else {
        unreachable!()
    };
    for path in retained_life
        .retained
        .iter_mut()
        .map(|row| &mut row.path)
        .chain(
            retained_life
                .nominal_arguments
                .iter_mut()
                .map(|row| &mut row.path),
        )
    {
        if let Some(zil_ir::LifeTypeProjection::Parameter(index)) = path.first_mut() {
            b.lookup(origins.len(), "Task return worker coordinate")?;
            let Some(parameter) = origins.get(index) else {
                return Ok(false);
            };
            *index = *parameter;
        }
    }
    Ok(projected == f.return_type)
}
