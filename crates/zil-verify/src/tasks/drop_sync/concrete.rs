//! Independently select every real caller input / environment construction,
//! then walk its SSA producers backwards. Lifetime rows provide no frame site.
use super::*;
use zil_ir::{ContractProjection, StoragePosition};

pub(super) fn settle(
    module: &TypedModule,
    function: FunctionId,
    symbolic: &Origins,
    b: &mut Budget,
) -> Check<Origins> {
    expand(module, function, symbolic, &mut BTreeSet::new(), b)
}

fn expand(
    module: &TypedModule,
    function: FunctionId,
    symbolic: &Origins,
    active: &mut BTreeSet<FunctionId>,
    b: &mut Budget,
) -> Check<Origins> {
    let mut result = Origins::default();
    result.add(symbolic, b)?;
    if result.parameters.is_empty() && result.selected.is_empty() {
        return Ok(result);
    }
    b.spend(active.len().saturating_add(1))?;
    if active.len() >= b.limits.max_call_depth || !active.insert(function) {
        return Ok(Origins {
            unknown: true,
            ..Origins::default()
        });
    }
    let mut positions = BTreeSet::new();
    for parameter in &result.parameters {
        b.spend(1)?;
        positions.insert(StoragePosition {
            base: StorageBase::Parameter(*parameter),
            path: vec![],
        });
    }
    for position in &result.selected {
        b.spend(position.path.len().saturating_add(1))?;
        positions.insert(position.clone());
    }
    result.parameters.clear();
    result.selected.clear();
    let definition = module
        .functions
        .get(function.0 as usize)
        .ok_or_else(|| reject("E-VERIFY-TASK", None, "actual cleanup function is absent"))?;
    for table in &module.dyn_vtables {
        b.spend(table.entries.len().saturating_add(1))?;
        if table
            .entries
            .iter()
            .chain(table.cleanup.iter())
            .any(|e| e.function == function)
        {
            result.unknown = true;
        }
    }
    for position in positions {
        let mut saw = false;
        for caller in &module.functions {
            b.spend(1)?;
            if caller.foreign.is_some() {
                continue;
            }
            let live = live_blocks(caller, b)?;
            for (block, reachable) in caller.blocks.iter().zip(&live) {
                if !reachable {
                    continue;
                }
                for instruction in &block.instructions {
                    b.spend(1)?;
                    if matches!(instruction.kind, InstructionKind::FunctionRef { function: f }
                        | InstructionKind::ForeignFunctionRef { function: f } if f == function)
                    {
                        result.unknown = true;
                    }
                    let selected = match position.base {
                        StorageBase::Parameter(index) => {
                            let arguments = match &instruction.kind {
                                InstructionKind::Call {
                                    function: target,
                                    arguments,
                                }
                                | InstructionKind::SuspendCall {
                                    function: target,
                                    arguments,
                                } if *target == function => Some(arguments.as_slice()),
                                InstructionKind::TaskCreate {
                                    worker, arguments, ..
                                }
                                | InstructionKind::TaskCreateOwned {
                                    worker, arguments, ..
                                } if *worker == function => Some(arguments.as_slice()),
                                _ => crate::stored_callables::called(
                                    module,
                                    caller,
                                    &instruction.kind,
                                )
                                .and_then(|(d, arguments)| {
                                    (d.function == function).then_some(arguments)
                                }),
                            };
                            arguments
                                .map(|arguments| {
                                    let value =
                                        *arguments.get(index as usize).ok_or_else(|| {
                                            reject(
                                                "E-VERIFY-TASK",
                                                Some(instruction.span),
                                                "actual cleanup argument is absent",
                                            )
                                        })?;
                                    selected::endpoint(module, caller, value, &position.path, b)
                                })
                                .transpose()?
                        }
                        StorageBase::Capture(index) => {
                            if let InstructionKind::MakeClosure { closure, .. } = instruction.kind
                                && definition.environment == Some(closure)
                            {
                                let value = instruction.result.ok_or_else(|| {
                                    reject(
                                        "E-VERIFY-TASK",
                                        Some(instruction.span),
                                        "actual cleanup environment has no SSA value",
                                    )
                                })?;
                                b.spend(position.path.len().saturating_add(1))?;
                                let mut path = vec![ContractProjection::Capture(index)];
                                path.extend_from_slice(&position.path);
                                Some(selected::endpoint(module, caller, value, &path, b)?)
                            } else {
                                None
                            }
                        }
                        _ => {
                            result.unknown = true;
                            None
                        }
                    };
                    if let Some(selected) = selected {
                        saw = true;
                        result.add(&expand(module, caller.id, &selected, active, b)?, b)?;
                    }
                }
            }
        }
        if !saw {
            result.unknown = true;
        }
    }
    active.remove(&function);
    Ok(result)
}
