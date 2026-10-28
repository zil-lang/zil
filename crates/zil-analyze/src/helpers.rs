//! Producer-side finite helper envelopes. A topological path propagation checks
//! claims; independent Verify uses its own provenance and trace reconstruction.
use super::*;

fn env(ty: &Type) -> Option<(ClosureId, Option<bool>)> {
    match ty {
        Type::Closure(id) => Some((*id, None)),
        Type::Address { mutable, inner } => match inner.as_ref() {
            Type::Closure(id) => Some((*id, Some(*mutable))),
            _ => None,
        },
        _ => None,
    }
}

// This phase reconstructs actual value origins itself. Bit 1 is the callback
// parameter, bit 2 an already checked factory result, and bit 4 an unsupported
// producer. Unions are conservative across stores and control-flow merges.
fn origins(
    module: &TypedModule,
    function: &TypedFunction,
    parameter: LocalId,
    budget: &mut Budget,
) -> CompileResult<(Vec<u8>, Vec<u8>)> {
    budget.spend(
        function.locals.len().saturating_add(function.values.len()),
        "helper environment origin tables",
    )?;
    let mut positions = vec![0u8; function.locals.len()];
    let mut results = vec![0u8; function.values.len()];
    positions[parameter.0 as usize] = 1;
    for block in &function.blocks {
        budget.spend(1, "helper environment producer block")?;
        for instruction in &block.instructions {
            budget.spend(1, "helper environment producer")?;
            let Some(result) = instruction.result else {
                continue;
            };
            let ty = &function.values[result.0 as usize];
            if env(ty).is_none() {
                continue;
            }
            results[result.0 as usize] = match instruction.kind {
                InstructionKind::Load(_)
                | InstructionKind::TakeLocal { .. }
                | InstructionKind::BorrowLocal { .. }
                | InstructionKind::BorrowLocalMut { .. }
                | InstructionKind::ReborrowAddress { .. } => 0,
                InstructionKind::Call {
                    function: callee, ..
                } => {
                    let producer = module
                        .functions
                        .get(callee.0 as usize)
                        .ok_or_else(|| bad("helper factory producer is absent"))?;
                    // retained::check_function has independently checked every
                    // factory before helpers run. Presence alone is not proof.
                    if producer.callable_return_contract.as_ref().is_some_and(|c| {
                        *ty == Type::Closure(c.closure) && producer.return_type == *ty
                    }) {
                        2
                    } else {
                        4
                    }
                }
                _ => 4,
            };
        }
    }
    loop {
        let mut changed = false;
        for block in &function.blocks {
            budget.spend(1, "helper environment provenance block")?;
            for instruction in &block.instructions {
                budget.spend(1, "helper environment provenance")?;
                let derived = match instruction.kind {
                    InstructionKind::Load(local)
                    | InstructionKind::TakeLocal { local }
                    | InstructionKind::BorrowLocal { local }
                    | InstructionKind::BorrowLocalMut { local } => positions[local.0 as usize],
                    InstructionKind::ReborrowAddress { address, .. } => results[address.0 as usize],
                    InstructionKind::Store { local, value } => {
                        let old = positions[local.0 as usize];
                        positions[local.0 as usize] |= results[value.0 as usize];
                        changed |= old != positions[local.0 as usize];
                        0
                    }
                    _ => 0,
                };
                if let Some(value) = instruction.result {
                    let old = results[value.0 as usize];
                    results[value.0 as usize] |= derived;
                    changed |= old != results[value.0 as usize];
                }
            }
        }
        if !changed {
            break;
        }
    }
    Ok((positions, results))
}

fn parameter_invocation(origin: u8, borrowed: bool) -> CompileResult<bool> {
    match (origin, borrowed) {
        (1, _) => Ok(true),
        (2, false) => Ok(false),
        _ => Err(vec![Diagnostic::unsupported(
            Phase::Analyze,
            "helper callable origin is unknown, mixed, or a borrowed local factory result",
            None,
        )]),
    }
}

pub(super) fn check(module: &TypedModule, budget: &mut Budget) -> CompileResult<()> {
    for function in &module.functions {
        let candidates = function
            .parameters
            .iter()
            .filter(|p| env(&function.locals[p.0 as usize].ty).is_some())
            .copied()
            .collect::<Vec<_>>();
        if candidates.is_empty() && function.callable_contract.is_none() {
            continue;
        }
        budget.spend(
            function.parameters.len().saturating_add(1),
            "helper parameter envelope",
        )?;
        if function.callable_contract.is_none()
            && candidates.len() == 1
            && function.environment.is_none()
            && function.foreign.is_none()
            && let Type::Closure(id) = function.locals[candidates[0].0 as usize].ty
            && matches!(&function.return_type,Type::Package{fields,..}
                if fields.iter().any(|(_,ty)| *ty==Type::Closure(id)))
        {
            callable_fields::self_contained(module, id, budget)?;
            continue;
        }
        let claim = function
            .callable_contract
            .as_ref()
            .ok_or_else(|| bad("callable parameter has no public helper contract"))?;
        let CallableTraces::Finite(claim_traces) = &claim.traces else {
            return Err(vec![Diagnostic::unsupported(
                Phase::Analyze,
                "cyclic callable Flow semantics are not implemented",
                Some(function.span),
            )]);
        };
        if candidates.len() != 1 || function.environment.is_some() || function.foreign.is_some() {
            return Err(bad("helper requires one concrete environment parameter"));
        }
        let local = *function
            .parameters
            .get(claim.parameter as usize)
            .ok_or_else(|| bad("helper parameter index outside signature"))?;
        if candidates[0] != local {
            return Err(bad("helper contract names a different parameter"));
        }
        if function.parameters.iter().any(|p| {
            *p != local
                && !function.locals[p.0 as usize].ty.is_copy_value()
                && !matches!(&function.locals[p.0 as usize].ty, Type::Desc(inner) if inner.as_ref() == &Type::ByteBuffer)
                && !matches!(
                    function.locals[p.0 as usize].ty,
                    Type::Managed(_) | Type::Package { .. }
                )
        }) {
            return Err(vec![Diagnostic::unsupported(
                Phase::Analyze,
                "helper extra parameters require finite Copy, Managed, Package or ByteBuffer description values",
                None,
            )]);
        }
        let (closure, borrowed) =
            env(&function.locals[local.0 as usize].ty).expect("classified parameter");
        let definition = module
            .closures
            .get(closure.0 as usize)
            .ok_or_else(|| bad("helper environment definition missing"))?;
        if !(function.return_type.is_copy_value()
            || (borrowed.is_some()
                && !definition.lending_captures.is_empty()
                && matches!(&function.return_type, Type::Address { inner, mutable: false }
                    if inner.is_copy_value() || matches!(&**inner, Type::Slice { element } if element.is_copy_value()))))
        {
            return Err(bad(
                "helper result requires finite Copy or a shared result from its borrowed environment",
            ));
        }
        if closure != claim.closure
            || claim.cleanup
                != if borrowed.is_some() {
                    CallableCleanup::Borrowed
                } else {
                    CallableCleanup::Owned
                }
        {
            return Err(bad("helper environment identity or cleanup mode differs"));
        }
        if !matches!(
            (definition.kind, claim.kind),
            (
                CallableKind::Fn,
                CallableKind::Fn | CallableKind::FnMut | CallableKind::FnOnce
            ) | (
                CallableKind::FnMut,
                CallableKind::FnMut | CallableKind::FnOnce
            ) | (CallableKind::FnOnce, CallableKind::FnOnce)
        ) || (claim.kind == CallableKind::FnOnce && borrowed.is_some())
            || (claim.kind == CallableKind::FnMut && borrowed != Some(true))
        {
            return Err(bad(
                "helper parameter cannot provide the requested call mode",
            ));
        }
        let mut hold = Vec::new();
        let mut call = Vec::new();
        let mut writes = Vec::new();
        for (i, capture) in definition.captures.iter().enumerate() {
            budget.spend(1, "helper capture envelope")?;
            if matches!(capture, Type::Address { .. }) {
                hold.push(i as u32);
            }
            if matches!(capture, Type::Address { .. } | Type::Desc(_)) {
                call.push(i as u32);
            }
            if matches!(capture, Type::Address { mutable: true, .. } | Type::Desc(_)) {
                writes.push(i as u32);
            }
        }
        if claim.hold != hold || claim.call != call || claim.writes != writes {
            return Err(bad(
                "helper Hold/Call/write envelope differs from its concrete captures",
            ));
        }
        let (locals, values) = origins(module, function, local, budget)?;
        let n = function.blocks.len();
        budget.product(n, 4, 1, "helper finite trace CFG")?;
        let mut reachable = vec![false; n];
        let mut edges = vec![Vec::new(); n];
        let mut stack = vec![function.entry.0 as usize];
        while let Some(b) = stack.pop() {
            budget.spend(1, "helper reachability")?;
            if b >= n {
                return Err(bad("helper CFG edge outside table"));
            }
            if reachable[b] {
                continue;
            }
            reachable[b] = true;
            edges[b] = match function.blocks[b].terminator {
                Terminator::Jump(b) => vec![b.0 as usize],
                Terminator::Branch {
                    then_block,
                    else_block,
                    ..
                } => vec![then_block.0 as usize, else_block.0 as usize],
                _ => Vec::new(),
            };
            stack.extend(&edges[b]);
        }
        let mut incoming = vec![0usize; n];
        for (b, outgoing) in edges.iter().enumerate() {
            if reachable[b] {
                for target in outgoing {
                    incoming[*target] += 1;
                }
            }
        }
        let mut queue: VecDeque<_> = (0..n)
            .filter(|b| reachable[*b] && incoming[*b] == 0)
            .collect();
        // Only the declared call kind is admitted below. Counts retain the
        // exact sequence while avoiding repeated long vector propagation.
        let mut prefixes = vec![BTreeSet::<usize>::new(); n];
        prefixes[function.entry.0 as usize].insert(0);
        let mut traces = BTreeSet::new();
        let mut processed = 0usize;
        while let Some(b) = queue.pop_front() {
            processed += 1;
            let mut suffix = 0usize;
            for instruction in &function.blocks[b].instructions {
                budget.spend(1, "helper finite trace instruction")?;
                let callback = match &instruction.kind {
                    InstructionKind::CallClosure { callee, .. }
                    | InstructionKind::CallClosureOnce { callee, .. } => {
                        if !parameter_invocation(values[callee.0 as usize], false)? {
                            continue;
                        }
                        Some(
                            if matches!(instruction.kind, InstructionKind::CallClosureOnce { .. }) {
                                CallableKind::FnOnce
                            } else {
                                CallableKind::Fn
                            },
                        )
                    }
                    InstructionKind::CallClosureShared { callee, .. }
                    | InstructionKind::CallClosureMut { callee, .. } => {
                        if !parameter_invocation(locals[callee.0 as usize], false)? {
                            continue;
                        }
                        Some(
                            if matches!(instruction.kind, InstructionKind::CallClosureMut { .. }) {
                                CallableKind::FnMut
                            } else {
                                CallableKind::Fn
                            },
                        )
                    }
                    InstructionKind::CallClosureBorrowed { callee, kind, .. } => {
                        parameter_invocation(values[callee.0 as usize], true)?;
                        Some(*kind)
                    }
                    InstructionKind::DropClosure { local }
                        if borrowed.is_some() && locals[local.0 as usize] & 1 != 0 =>
                    {
                        return Err(bad("borrowed helper cannot destroy its environment"));
                    }
                    InstructionKind::MakeClosure { .. } => {
                        return Err(bad(
                            "helper-local nested environments require an additional contract",
                        ));
                    }
                    _ => None,
                };
                if let Some(kind) = callback {
                    if kind != claim.kind {
                        return Err(bad("helper callback call mode differs from its bound"));
                    }
                    suffix = suffix.saturating_add(1);
                }
            }
            let mut output = BTreeSet::new();
            for prefix in &prefixes[b] {
                budget.spend(1, "helper exact call-count concatenation")?;
                let calls = prefix.saturating_add(suffix);
                if claim.kind == CallableKind::FnOnce && calls > 1 {
                    return Err(bad("helper exceeds its finite call allowance"));
                }
                budget.lookup(output.len(), "helper exact call-count insertion")?;
                output.insert(calls);
            }
            let exit = match function.blocks[b].terminator {
                Terminator::Return(_) => Some(CallableExit::Return),
                Terminator::Trap => Some(CallableExit::Trap),
                Terminator::Unreachable => {
                    return Err(bad("helper has a reachable undefined exit"));
                }
                _ => None,
            };
            if let Some(exit) = exit {
                for calls in output {
                    budget.lookup(traces.len(), "helper exact call-count exit")?;
                    traces.insert((calls, exit));
                }
            } else {
                for target in &edges[b] {
                    budget.spend(output.len(), "helper trace propagation")?;
                    for count in &output {
                        budget.lookup(prefixes[*target].len(), "helper exact call-count union")?;
                        prefixes[*target].insert(*count);
                    }
                    incoming[*target] -= 1;
                    if incoming[*target] == 0 {
                        queue.push_back(*target);
                    }
                }
            }
        }
        if processed != reachable.iter().filter(|r| **r).count() {
            return Err(bad("cyclic helper requires an explicit loop contract"));
        }
        budget.spend(claim_traces.len(), "helper public trace comparison")?;
        if traces.len() != claim_traces.len() {
            return Err(bad("public helper trace claim differs from its CFG"));
        }
        for ((count, exit), claim_trace) in traces.into_iter().zip(claim_traces) {
            budget.spend(claim_trace.calls.len(), "helper public call comparison")?;
            if count != claim_trace.calls.len()
                || exit != claim_trace.exit
                || claim_trace.calls.iter().any(|kind| *kind != claim.kind)
            {
                return Err(bad("public helper trace claim differs from its CFG"));
            }
        }
    }
    Ok(())
}
