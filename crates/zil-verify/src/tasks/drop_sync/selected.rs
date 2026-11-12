//! Backward selection of real owning frame leaves. These selectors arise from
//! executable Return/constructor transfers, never from lifetime or port rows.
use super::*;
use zil_ir::{ContractProjection, PlaceProjection, StoragePosition};

fn unknown() -> Origins {
    Origins {
        unknown: true,
        ..Origins::default()
    }
}
fn depth(work: usize, b: &mut Budget) -> Check<()> {
    if work > b.limits.max_call_depth {
        return Err(limit("selected Task frame origin depth exceeded"));
    }
    b.spend(work.saturating_add(1))
}

pub(super) fn invocation(
    module: &TypedModule,
    f: &TypedFunction,
    callee: ValueId,
    args: &[ValueId],
    functions: &mut BTreeSet<FunctionId>,
    b: &mut Budget,
) -> Check<Origins> {
    let Type::Closure(id) = &f.values[callee.0 as usize] else {
        return Ok(unknown());
    };
    let d = module
        .closures
        .get(id.0 as usize)
        .filter(|d| d.kind == CallableKind::FnOnce)
        .ok_or_else(|| {
            reject(
                "E-VERIFY-TASK",
                None,
                "selected frame call has no actual consuming entry",
            )
        })?;
    let summary = returns(module, d.function, &[], functions, b, 0)?;
    instantiate(module, f, args, Some(callee), &summary, functions, b, 0)
}

pub(super) fn sources(
    module: &TypedModule,
    f: &TypedFunction,
    args: &[ValueId],
    positions: &BTreeSet<StoragePosition>,
    functions: &mut BTreeSet<FunctionId>,
    b: &mut Budget,
) -> Check<Origins> {
    for position in positions {
        b.spend(position.path.len().saturating_add(1))?;
    }
    let symbolic = Origins {
        selected: positions.clone(),
        ..Origins::default()
    };
    instantiate(module, f, args, None, &symbolic, functions, b, 0)
}

#[allow(clippy::too_many_arguments)]
fn instantiate(
    module: &TypedModule,
    f: &TypedFunction,
    args: &[ValueId],
    environment: Option<ValueId>,
    summary: &Origins,
    functions: &mut BTreeSet<FunctionId>,
    b: &mut Budget,
    n: usize,
) -> Check<Origins> {
    depth(n, b)?;
    b.spend(
        summary
            .workers
            .len()
            .saturating_add(summary.parameters.len()),
    )?;
    let mut out = Origins {
        unknown: summary.unknown,
        workers: summary.workers.clone(),
        ..Origins::default()
    };
    let reachable = live_blocks(f, b)?;
    for parameter in &summary.parameters {
        let value = *args.get(*parameter as usize).ok_or_else(|| {
            reject(
                "E-VERIFY-TASK",
                None,
                "selected frame actual argument absent",
            )
        })?;
        out.add(
            &value_at(module, f, &reachable, value, &[], functions, b, n + 1)?,
            b,
        )?;
    }
    for position in &summary.selected {
        b.spend(position.path.len().saturating_add(1))?;
        let selected = match position.base {
            StorageBase::Parameter(index) => {
                let value = *args.get(index as usize).ok_or_else(|| {
                    reject(
                        "E-VERIFY-TASK",
                        None,
                        "selected frame source argument absent",
                    )
                })?;
                value_at(
                    module,
                    f,
                    &reachable,
                    value,
                    &position.path,
                    functions,
                    b,
                    n + 1,
                )?
            }
            StorageBase::Capture(index) => {
                if let Some(value) = environment {
                    let mut path = vec![ContractProjection::Capture(index)];
                    path.extend_from_slice(&position.path);
                    value_at(module, f, &reachable, value, &path, functions, b, n + 1)?
                } else {
                    unknown()
                }
            }
            _ => unknown(),
        };
        out.add(&selected, b)?;
    }
    Ok(out)
}

fn returns(
    module: &TypedModule,
    id: FunctionId,
    path: &[ContractProjection],
    functions: &mut BTreeSet<FunctionId>,
    b: &mut Budget,
    n: usize,
) -> Check<Origins> {
    depth(n, b)?;
    if !functions.insert(id) {
        return Ok(unknown());
    }
    let f = module
        .functions
        .get(id.0 as usize)
        .ok_or_else(|| reject("E-VERIFY-TASK", None, "selected frame entry absent"))?;
    if f.foreign.is_some() {
        functions.remove(&id);
        return Ok(unknown());
    }
    let live = live_blocks(f, b)?;
    let mut out = Origins::default();
    let mut saw = false;
    for (block, reached) in f.blocks.iter().zip(&live) {
        b.spend(1)?;
        if *reached && let Terminator::Return(Some(value)) = block.terminator {
            saw = true;
            out.add(
                &value_at(module, f, &live, value, path, functions, b, n + 1)?,
                b,
            )?;
        }
    }
    functions.remove(&id);
    if !saw {
        out.unknown = true;
    }
    Ok(out)
}

#[allow(clippy::too_many_arguments)]
fn value_at(
    module: &TypedModule,
    f: &TypedFunction,
    live: &[bool],
    value: ValueId,
    path: &[ContractProjection],
    functions: &mut BTreeSet<FunctionId>,
    b: &mut Budget,
    n: usize,
) -> Check<Origins> {
    depth(n, b)?;
    b.spend(path.len())?;
    if path
        .iter()
        .any(|step| matches!(step, ContractProjection::RetainedPort(_)))
    {
        return Ok(unknown());
    }
    if path.is_empty() {
        return returned_value(
            module,
            f,
            live,
            value,
            functions,
            &mut BTreeSet::new(),
            &mut BTreeSet::new(),
            b,
        );
    }
    let mut producer = None;
    for (block, reached) in f.blocks.iter().zip(live) {
        b.spend(block.instructions.len().saturating_add(1))?;
        if !reached {
            continue;
        }
        for instruction in &block.instructions {
            if instruction.result == Some(value) && producer.replace(&instruction.kind).is_some() {
                return Err(reject(
                    "E-VERIFY-TASK",
                    Some(instruction.span),
                    "selected Task frame SSA definition repeats",
                ));
            }
        }
    }
    let (first, tail) = path.split_first().expect("nonempty selection");
    match producer {
        Some(InstructionKind::MakeClosure { captures, .. }) => {
            let ContractProjection::Capture(index) = first else {
                return Ok(unknown());
            };
            let child = *captures
                .get(*index as usize)
                .ok_or_else(|| reject("E-VERIFY-TASK", None, "selected Task capture absent"))?;
            value_at(module, f, live, child, tail, functions, b, n + 1)
        }
        Some(
            InstructionKind::MakePackage { fields }
            | InstructionKind::MakeAggregate { fields }
            | InstructionKind::MakeOwnedTuple { fields },
        ) => {
            let ContractProjection::Place(PlaceProjection::Field(index)) = first else {
                return Ok(unknown());
            };
            let child = *fields.get(*index as usize).ok_or_else(|| {
                reject("E-VERIFY-TASK", None, "selected Task owning field absent")
            })?;
            value_at(module, f, live, child, tail, functions, b, n + 1)
        }
        Some(InstructionKind::Load(local) | InstructionKind::TakeLocal { local }) => {
            if let Some(index) = f.parameters.iter().position(|p| p == local) {
                return Ok(Origins {
                    selected: BTreeSet::from([StoragePosition {
                        base: StorageBase::Parameter(index as u32),
                        path: path.to_vec(),
                    }]),
                    ..Origins::default()
                });
            }
            let mut out = Origins::default();
            let mut saw = false;
            for (block, reached) in f.blocks.iter().zip(live) {
                b.spend(block.instructions.len().saturating_add(1))?;
                if !reached {
                    continue;
                }
                for instruction in &block.instructions {
                    if let InstructionKind::Store {
                        local: target,
                        value,
                    } = instruction.kind
                        && target == *local
                    {
                        saw = true;
                        out.add(
                            &value_at(module, f, live, value, path, functions, b, n + 1)?,
                            b,
                        )?;
                    }
                }
            }
            if !saw {
                out.unknown = true;
            }
            Ok(out)
        }
        Some(InstructionKind::Call {
            function,
            arguments,
        }) => {
            let summary = returns(module, *function, path, functions, b, n + 1)?;
            instantiate(module, f, arguments, None, &summary, functions, b, n + 1)
        }
        Some(InstructionKind::CallClosureOnce { callee, arguments }) => {
            let Type::Closure(id) = &f.values[callee.0 as usize] else {
                return Ok(unknown());
            };
            let d = module
                .closures
                .get(id.0 as usize)
                .filter(|d| d.kind == CallableKind::FnOnce)
                .ok_or_else(|| {
                    reject(
                        "E-VERIFY-TASK",
                        None,
                        "selected returned environment is not consuming",
                    )
                })?;
            let summary = returns(module, d.function, path, functions, b, n + 1)?;
            instantiate(
                module,
                f,
                arguments,
                Some(*callee),
                &summary,
                functions,
                b,
                n + 1,
            )
        }
        Some(InstructionKind::TakeCapture { index }) => Ok(Origins {
            selected: BTreeSet::from([StoragePosition {
                base: StorageBase::Capture(*index),
                path: path.to_vec(),
            }]),
            ..Origins::default()
        }),
        // RetainedPort is a data dependency, never a frame allocation selector.
        _ => Ok(unknown()),
    }
}
