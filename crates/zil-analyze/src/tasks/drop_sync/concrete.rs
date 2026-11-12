//! Close symbolic cleanup obligations using forward facts from every real
//! caller and constructor. A Task's result type supplies no worker evidence.
use super::*;

pub(super) fn settle(
    module: &TypedModule,
    function: FunctionId,
    symbolic: &Origins,
    b: &mut Budget,
) -> CompileResult<Origins> {
    close(module, function, symbolic, &mut BTreeSet::new(), b)
}

fn close(
    module: &TypedModule,
    function: FunctionId,
    symbolic: &Origins,
    visiting: &mut BTreeSet<FunctionId>,
    b: &mut Budget,
) -> CompileResult<Origins> {
    if symbolic.parameters.is_empty() && symbolic.captures.is_empty() {
        return symbolic.snapshot(b);
    }
    symbolic.charge(b)?;
    b.spend(
        visiting.len().saturating_add(1),
        "actual cleanup caller activation",
    )?;
    if visiting.len() >= b.helper_depth || !visiting.insert(function) {
        return Ok(Origins {
            unknown: true,
            ..Origins::default()
        });
    }
    let entry = module
        .functions
        .get(function.0 as usize)
        .ok_or_else(|| fail(Default::default(), "cleanup source entry is absent"))?;
    let mut result = Origins {
        unknown: symbolic.unknown,
        vacant: symbolic.vacant,
        workers: symbolic.workers.clone(),
        ..Origins::default()
    };
    let mut bound_parameters = BTreeSet::new();
    let mut bound_captures = BTreeSet::new();
    for table in &module.dyn_vtables {
        b.spend(
            table.entries.len().saturating_add(1),
            "cleanup indirect entry inventory",
        )?;
        if table
            .entries
            .iter()
            .chain(table.cleanup.iter())
            .any(|e| e.function == function)
        {
            result.unknown = true;
        }
    }
    for caller in &module.functions {
        b.spend(1, "cleanup actual caller")?;
        if caller.foreign.is_some() {
            continue;
        }
        let reachable = reachable_blocks(caller, b)?;
        let mut facts = None;
        for (block, reachable) in caller.blocks.iter().zip(&reachable) {
            if !reachable {
                continue;
            }
            for i in &block.instructions {
                b.spend(1, "cleanup caller instruction")?;
                if matches!(i.kind, InstructionKind::FunctionRef { function: f }
                    | InstructionKind::ForeignFunctionRef { function: f } if f == function)
                {
                    result.unknown = true;
                }
                let arguments = match &i.kind {
                    InstructionKind::Call {
                        function: f,
                        arguments,
                    }
                    | InstructionKind::SuspendCall {
                        function: f,
                        arguments,
                    } if *f == function => Some(arguments.as_slice()),
                    InstructionKind::TaskCreate { worker, arguments }
                    | InstructionKind::TaskCreateOwned { worker, arguments }
                        if *worker == function =>
                    {
                        Some(arguments.as_slice())
                    }
                    _ => crate::stored_callables::called(module, caller, &i.kind)
                        .and_then(|(d, arguments)| (d.function == function).then_some(arguments)),
                };
                let captures = match &i.kind {
                    InstructionKind::MakeClosure { closure, captures }
                        if entry.environment == Some(*closure) =>
                    {
                        Some(captures.as_slice())
                    }
                    _ => None,
                };
                if arguments.is_none() && captures.is_none() {
                    continue;
                }
                if facts.is_none() {
                    facts = Some(infer_origins(
                        module,
                        caller,
                        b,
                        &mut BTreeSet::from([caller.id]),
                        &mut BTreeMap::new(),
                    )?);
                }
                let (values, _) = facts.as_ref().expect("actual forward frame facts");
                for (sources, bindings, required) in [
                    (arguments, &mut bound_parameters, &symbolic.parameters),
                    (captures, &mut bound_captures, &symbolic.captures),
                ] {
                    let Some(sources) = sources else {
                        continue;
                    };
                    b.spend(required.len(), "cleanup original source ordinals")?;
                    for index in required {
                        let source = sources
                            .get(*index as usize)
                            .and_then(|id| values.get(id.0 as usize))
                            .ok_or_else(|| fail(i.span, "cleanup actual input is absent"))?;
                        let closed = close(module, caller.id, source, visiting, b)?;
                        result.add(&closed, b)?;
                        b.spend(1, "cleanup actual source binding")?;
                        bindings.insert(*index);
                    }
                }
            }
        }
    }
    b.spend(
        symbolic
            .parameters
            .len()
            .saturating_add(symbolic.captures.len()),
        "cleanup complete input coverage",
    )?;
    if symbolic.parameters != bound_parameters || symbolic.captures != bound_captures {
        result.unknown = true;
    }
    visiting.remove(&function);
    Ok(result)
}
