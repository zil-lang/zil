//! Independent checking of finite concrete callback contracts. The public
//! candidate is compared with a CFG-derived finite language of calls. No
//! producer solver or callee body inlining supplies authority to a caller.
use super::*;
use std::collections::VecDeque;

fn bad(f: &TypedFunction, message: impl Into<String>) -> Diagnostic {
    reject("E-VERIFY-CALLABLE-CONTRACT", Some(f.span), message)
}

fn count_tree_work(size: usize) -> usize {
    size.checked_ilog2().map_or(1, |n| n as usize + 2)
}

pub(super) fn parameter_type(ty: &Type) -> Option<(ClosureId, bool)> {
    match ty {
        Type::Closure(id) => Some((*id, false)),
        Type::Address { inner, .. } => match inner.as_ref() {
            Type::Closure(id) => Some((*id, true)),
            _ => None,
        },
        _ => None,
    }
}

struct EnvironmentOrigins {
    values_at: usize,
    marks: Vec<u8>,
}

/// Independent provenance graph. Edges describe only actual local/SSA
/// transfers; type equality never supplies an edge. The finite marks are
/// parameter (1), verified self-contained factory (2), and other producer (4).
fn environment_origins(
    module: &TypedModule,
    f: &TypedFunction,
    parameter: LocalId,
    budget: &mut Budget,
) -> Check<EnvironmentOrigins> {
    let values_at = f.locals.len();
    let count = values_at.saturating_add(f.values.len());
    budget.spend(count.saturating_mul(2))?;
    let mut marks = vec![0u8; count];
    let mut edges = vec![Vec::<usize>::new(); count];
    let mut pending = VecDeque::new();
    marks[parameter.0 as usize] = 1;
    budget.spend(1)?;
    pending.push_back(parameter.0 as usize);
    for block in &f.blocks {
        budget.spend(1)?;
        for instruction in &block.instructions {
            budget.spend(1)?;
            if let InstructionKind::Store { local, value } = instruction.kind {
                budget.spend(1)?;
                edges[values_at + value.0 as usize].push(local.0 as usize);
            }
            let Some(result) = instruction.result else {
                continue;
            };
            if parameter_type(&f.values[result.0 as usize]).is_none() {
                continue;
            }
            let destination = values_at + result.0 as usize;
            let origin = match instruction.kind {
                InstructionKind::Load(local)
                | InstructionKind::TakeLocal { local }
                | InstructionKind::BorrowLocal { local }
                | InstructionKind::BorrowLocalMut { local } => Some(local.0 as usize),
                InstructionKind::ReborrowAddress { address, .. } => {
                    Some(values_at + address.0 as usize)
                }
                _ => None,
            };
            if let Some(origin) = origin {
                budget.spend(1)?;
                edges[origin].push(destination);
                continue;
            }
            // All return claims and bodies were independently checked by
            // retained before this pass. Do not infer safety from a type ID or
            // from a producer-supplied empty envelope alone.
            let factory = match instruction.kind {
                InstructionKind::Call { function, .. } => {
                    let producer = &module.functions[function.0 as usize];
                    producer.callable_return_contract.as_ref().is_some_and(|c| {
                        f.values[result.0 as usize] == Type::Closure(c.closure)
                            && producer.return_type == Type::Closure(c.closure)
                    })
                }
                _ => false,
            };
            marks[destination] = if factory { 2 } else { 4 };
            budget.spend(1)?;
            pending.push_back(destination);
        }
    }
    while let Some(origin) = pending.pop_front() {
        budget.spend(1)?;
        for &destination in &edges[origin] {
            budget.spend(1)?;
            let merged = marks[destination] | marks[origin];
            if merged != marks[destination] {
                budget.spend(1)?;
                marks[destination] = merged;
                pending.push_back(destination);
            }
        }
    }
    Ok(EnvironmentOrigins { values_at, marks })
}

fn invocation(
    f: &TypedFunction,
    instruction: &Instruction,
    origins: &EnvironmentOrigins,
) -> Check<Option<CallableKind>> {
    let (origin, kind, borrowed) = match &instruction.kind {
        InstructionKind::CallClosureShared { callee, .. } => {
            (callee.0 as usize, CallableKind::Fn, false)
        }
        InstructionKind::CallClosureMut { callee, .. } => {
            (callee.0 as usize, CallableKind::FnMut, false)
        }
        InstructionKind::CallClosure { callee, .. } => (
            origins.values_at + callee.0 as usize,
            CallableKind::Fn,
            false,
        ),
        InstructionKind::CallClosureOnce { callee, .. } => (
            origins.values_at + callee.0 as usize,
            CallableKind::FnOnce,
            false,
        ),
        InstructionKind::CallClosureBorrowed { callee, kind, .. } => {
            (origins.values_at + callee.0 as usize, *kind, true)
        }
        _ => return Ok(None),
    };
    match (origins.marks[origin], borrowed) {
        (1, _) => Ok(Some(kind)),
        (2, false) => Ok(None),
        _ => Err(unsupported(
            "helper callable origin is unknown, mixed, or a borrowed local factory result",
            Some(f.span),
        )),
    }
}

fn dependency_order(
    module: &TypedModule,
    graphs: &[FunctionGraph],
    budget: &mut Budget,
) -> Check<Vec<usize>> {
    let count = module.functions.len();
    budget.spend(count.saturating_mul(4))?;
    let mut predecessors = vec![0usize; count];
    let mut consumers = vec![Vec::new(); count];
    for (caller, f) in module.functions.iter().enumerate() {
        if f.callable_contract.is_none() {
            continue;
        }
        budget.spend(f.blocks.len())?;
        for (b, block) in f.blocks.iter().enumerate() {
            if !graphs[caller].reachable[b] {
                continue;
            }
            for instruction in &block.instructions {
                budget.spend(1)?;
                if let InstructionKind::Call {
                    function: callee, ..
                } = instruction.kind
                {
                    let callee = callee.0 as usize;
                    if module.functions[callee].callable_contract.is_some() {
                        budget.spend(2)?;
                        predecessors[caller] += 1;
                        consumers[callee].push(caller);
                    }
                }
            }
        }
    }
    let mut frontier: VecDeque<_> = predecessors
        .iter()
        .enumerate()
        .filter_map(|(i, degree)| (*degree == 0).then_some(i))
        .collect();
    let mut order = Vec::with_capacity(count);
    while let Some(callee) = frontier.pop_front() {
        budget.spend(consumers[callee].len().saturating_add(1))?;
        order.push(callee);
        for &caller in &consumers[callee] {
            predecessors[caller] -= 1;
            if predecessors[caller] == 0 {
                frontier.push_back(caller);
            }
        }
    }
    if order.len() != count {
        return Err(unsupported(
            "recursive callable helper dependency requires an explicit finite contract",
            None,
        ));
    }
    Ok(order)
}

fn forwarded<'a>(
    module: &'a TypedModule,
    f: &TypedFunction,
    instruction: &Instruction,
    origins: &EnvironmentOrigins,
    kind: CallableKind,
    checked: &[bool],
    budget: &mut Budget,
) -> Check<Option<&'a [CallableTrace]>> {
    let InstructionKind::Call {
        function: callee,
        arguments,
    } = &instruction.kind
    else {
        return Ok(None);
    };
    budget.spend(arguments.len())?;
    let mut environment = None;
    for (index, value) in arguments.iter().enumerate() {
        if parameter_type(&f.values[value.0 as usize]).is_some()
            && environment.replace((index, *value)).is_some()
        {
            return Err(bad(f, "nested helper forwards more than one environment"));
        }
    }
    let Some((parameter, value)) = environment else {
        return Ok(None);
    };
    let target = &module.functions[callee.0 as usize];
    let claim = target
        .callable_contract
        .as_ref()
        .ok_or_else(|| bad(f, "forwarded helper has no finite contract"))?;
    if !checked[callee.0 as usize]
        || claim.parameter as usize != parameter
        || claim.kind != kind
        || origins.marks[origins.values_at + value.0 as usize] != 1
        || parameter_type(&f.values[value.0 as usize]).map(|(id, _)| id) != Some(claim.closure)
    {
        return Err(bad(
            f,
            "nested helper must forward its actual parameter to an independently checked matching contract",
        ));
    }
    let finite = claim.traces.as_finite().ok_or_else(|| {
        unsupported(
            "cyclic callable Flow semantics are not implemented",
            Some(instruction.span),
        )
    })?;
    Ok(Some(finite))
}

/// Each reachable block is processed after all its predecessors. Trace sets
/// merge equal prefixes, instead of enumerating duplicate syntactic paths.
/// Cycles are a reported scope limit, never an empty or truncated contract.
fn traces(
    module: &TypedModule,
    f: &TypedFunction,
    graph: &FunctionGraph,
    parameter: LocalId,
    expected_kind: CallableKind,
    checked: &[bool],
    budget: &mut Budget,
) -> Check<Vec<CallableTrace>> {
    let origins = environment_origins(module, f, parameter, budget)?;
    let n = f.blocks.len();
    budget.spend(n.saturating_mul(3))?;
    let mut degree = vec![0usize; n];
    let mut reachable_count = 0usize;
    for b in 0..n {
        if !graph.reachable[b] {
            continue;
        }
        reachable_count += 1;
        for &next in &graph.successors[b] {
            budget.spend(1)?;
            degree[next] += 1;
        }
    }
    let mut ready = VecDeque::new();
    for (b, incoming) in degree.iter().enumerate() {
        if graph.reachable[b] && *incoming == 0 {
            ready.push_back(b);
        }
    }
    let mut order = Vec::with_capacity(reachable_count);
    while let Some(b) = ready.pop_front() {
        budget.spend(1)?;
        order.push(b);
        for &next in &graph.successors[b] {
            budget.spend(1)?;
            degree[next] -= 1;
            if degree[next] == 0 {
                ready.push_back(next);
            }
        }
    }
    if order.len() != reachable_count {
        return Err(unsupported(
            "concrete callback helpers require an acyclic finite call-trace CFG",
            Some(f.span),
        ));
    }
    // An exact count is sufficient only because each actual invocation below
    // must have the same independently checked kind. Public traces stay full.
    let mut inputs: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); n];
    inputs[f.entry.0 as usize].insert(0);
    let mut exits = BTreeSet::<(usize, u8)>::new();
    for b in order {
        budget.spend(inputs[b].len().saturating_add(1))?;
        let mut states = std::mem::take(&mut inputs[b]);
        for instruction in &f.blocks[b].instructions {
            budget.spend(1)?;
            if matches!(instruction.kind, InstructionKind::MakeClosure { .. }) {
                return Err(unsupported(
                    "helper-local nested environments require an additional contract",
                    Some(instruction.span),
                ));
            }
            if let Some(children) = forwarded(
                module,
                f,
                instruction,
                &origins,
                expected_kind,
                checked,
                budget,
            )? {
                let mut returning = BTreeSet::new();
                for prefix in &states {
                    for child in children {
                        budget.spend(1)?;
                        let count = prefix
                            .checked_add(child.calls.len())
                            .ok_or_else(|| limit("nested helper call count overflow"))?;
                        if expected_kind == CallableKind::FnOnce && count > 1 {
                            return Err(bad(f, "helper consumes its callback more than once"));
                        }
                        if child.exit == CallableExit::Trap {
                            budget.spend(count_tree_work(exits.len()))?;
                            exits.insert((count, 1));
                        } else {
                            budget.spend(count_tree_work(returning.len()))?;
                            returning.insert(count);
                        }
                    }
                }
                states = returning;
                continue;
            }
            if let Some(kind) = invocation(f, instruction, &origins)? {
                if kind != expected_kind {
                    return Err(bad(
                        f,
                        "helper callback use differs from its requested bound",
                    ));
                }
                let mut next = BTreeSet::new();
                for count in states {
                    budget.spend(count_tree_work(next.len()))?;
                    if kind == CallableKind::FnOnce && count != 0 {
                        return Err(bad(f, "helper consumes its callback more than once"));
                    }
                    next.insert(
                        count
                            .checked_add(1)
                            .ok_or_else(|| limit("helper call count overflow"))?,
                    );
                }
                states = next;
            }
        }
        match f.blocks[b].terminator {
            Terminator::Return(_) | Terminator::Trap => {
                let exit = u8::from(matches!(f.blocks[b].terminator, Terminator::Trap));
                for trace in states {
                    budget.spend(count_tree_work(exits.len()))?;
                    exits.insert((trace, exit));
                }
            }
            _ => {
                for &next in &graph.successors[b] {
                    for trace in &states {
                        budget.spend(count_tree_work(inputs[next].len()))?;
                        inputs[next].insert(*trace);
                    }
                }
            }
        }
    }
    let mut result = Vec::new();
    for (count, exit) in exits {
        budget.spend(count.saturating_add(1))?;
        result.push(CallableTrace {
            calls: vec![expected_kind; count],
            exit: if exit == 0 {
                CallableExit::Return
            } else {
                CallableExit::Trap
            },
        });
    }
    Ok(result)
}

/// Bound raw metadata before the module fingerprint walks it. A raw Rust API
/// receives the same admission checks as decoded wire candidates.
pub(super) fn check_claim_shape(
    f: &TypedFunction,
    limits: &Limits,
    budget: &mut Budget,
) -> Check<()> {
    let Some(claim) = &f.callable_contract else {
        return Ok(());
    };
    let field_count = claim
        .hold
        .len()
        .saturating_add(claim.call.len())
        .saturating_add(claim.writes.len());
    budget.spend(field_count.saturating_add(2))?;
    if field_count > limits.max_nodes {
        return Err(limit(
            "raw callback contract metadata node allowance exhausted",
        ));
    }
    match &claim.traces {
        CallableTraces::Finite(traces) => {
            budget.spend(traces.len())?;
            if traces.len() > limits.max_nodes {
                return Err(limit(
                    "raw callback contract metadata node allowance exhausted",
                ));
            }
            for trace in traces {
                budget.spend(trace.calls.len().saturating_add(1))?;
                if trace.calls.len() > limits.max_nodes {
                    return Err(limit("raw callback trace node allowance exhausted"));
                }
            }
        }
        CallableTraces::Flow(flow) => {
            if flow.nodes.len() > limits.max_nodes {
                return Err(limit("raw callback flow node allowance exhausted"));
            }
            budget.spend(flow.nodes.len().saturating_add(1).saturating_mul(2))?;
            let (mut calls, mut edges) = (0usize, 0usize);
            for node in &flow.nodes {
                calls = calls
                    .checked_add(node.calls.len())
                    .filter(|n| *n <= limits.max_instructions)
                    .ok_or_else(|| limit("raw callback flow call allowance exhausted"))?;
                budget.spend(node.calls.len().saturating_add(1).saturating_mul(2))?;
                match &node.next {
                    CallableFlowNext::Edges(next) => {
                        edges = edges
                            .checked_add(next.len())
                            .filter(|n| *n <= limits.max_analysis_steps)
                            .ok_or_else(|| limit("raw callback flow edge allowance exhausted"))?;
                        budget.spend(next.len().saturating_add(1).saturating_mul(2))?;
                    }
                    CallableFlowNext::Exit(_) => budget.spend(2)?,
                }
            }
        }
    }
    Ok(())
}

pub(super) fn check_module(
    module: &TypedModule,
    graphs: &[FunctionGraph],
    budget: &mut Budget,
) -> Check<()> {
    let order = dependency_order(module, graphs, budget)?;
    budget.spend(module.functions.len())?;
    let mut checked = vec![false; module.functions.len()];
    for index in order {
        let f = &module.functions[index];
        let graph = &graphs[index];
        let mut parameter = None;
        for (index, local) in f.parameters.iter().enumerate() {
            let ty = &f.locals[local.0 as usize].ty;
            if let Some((closure, borrowed)) = parameter_type(ty) {
                budget.spend(1)?;
                if parameter
                    .replace((index as u32, closure, borrowed, ty))
                    .is_some()
                {
                    return Err(unsupported(
                        "a concrete helper supports exactly one callback environment parameter",
                        Some(f.span),
                    ));
                }
            }
        }
        let Some((parameter, closure, borrowed, ty)) = parameter else {
            if f.callable_contract.is_some() {
                return Err(bad(
                    f,
                    "non-helper function cannot claim a callback parameter contract",
                ));
            }
            continue;
        };
        if f.foreign.is_some() || f.environment.is_some() {
            return Err(bad(
                f,
                "a callback helper must be an ordinary checked function",
            ));
        }
        // A concrete self-contained input may enter ordinary owning State.
        // This is an owned value boundary; borrowed/descriptor environments
        // still require the separately reconstructed synchronous contract.
        if f.callable_contract.is_none()
            && !borrowed
            && matches!(&f.return_type, Type::Package { fields, .. }
                if fields.iter().any(|(_, ty)| *ty == Type::Closure(closure)))
        {
            closures::self_contained(module, closure, budget)?;
            packages::check_type_for(module, &f.return_type, Some(f.span), budget)?;
            continue;
        }
        let definition = closures::definition(module, closure, Some(f.span))?;
        if !f.return_type.is_copy_value()
            && !(borrowed
                && closures::lending_result(&f.return_type)
                && !definition.lending_captures.is_empty())
        {
            return Err(unsupported(
                "concrete callback helper results require finite Copy or a borrowed environment lending result",
                Some(f.span),
            ));
        }
        for (index, local) in f.parameters.iter().enumerate() {
            if index as u32 == parameter {
                continue;
            }
            let ty = &f.locals[local.0 as usize].ty;
            if !ty.is_copy_value()
                && !matches!(ty, Type::Address { inner, .. } if inner.is_copy_value())
                && !matches!(ty, Type::Managed(_) | Type::Package { .. })
                && !matches!(ty, Type::Desc(inner) if inner.as_ref() == &Type::ByteBuffer)
            {
                return Err(unsupported(
                    "other callback helper parameters must be finite Copy, Managed, complete Package or ByteBuffer description values",
                    Some(f.span),
                ));
            }
        }
        let Some(claim) = &f.callable_contract else {
            return Err(bad(
                f,
                "callback helper is missing its independently checkable contract",
            ));
        };
        let CallableTraces::Finite(claim_traces) = &claim.traces else {
            return Err(unsupported(
                "cyclic callable Flow semantics are not implemented",
                Some(f.span),
            ));
        };
        if !closures::supports(definition.kind, claim.kind) {
            return Err(bad(
                f,
                "requested callback bound is not supported by the concrete entry",
            ));
        }
        if !borrowed && claim.kind == CallableKind::FnMut {
            return Err(unsupported(
                "this helper slice takes FnMut through an exclusive environment address",
                Some(f.span),
            ));
        }
        if borrowed {
            let Type::Address { mutable, .. } = ty else {
                unreachable!()
            };
            if !(claim.kind == CallableKind::Fn && !mutable
                || claim.kind == CallableKind::FnMut && *mutable)
            {
                return Err(bad(
                    f,
                    "borrowed callback parameter permission must match its exact Fn/FnMut entry",
                ));
            }
        }
        let mut hold = Vec::new();
        let mut call = Vec::new();
        let mut writes = Vec::new();
        for (index, capture) in definition.captures.iter().enumerate() {
            budget.spend(1)?;
            if matches!(capture, Type::Address { .. }) {
                hold.push(index as u32);
            }
            if matches!(capture, Type::Address { .. } | Type::Desc(_)) {
                call.push(index as u32);
            }
            // This is a conservative capability upper bound, not a claim that
            // a write occurs. Unknown effects never become an empty summary.
            if matches!(capture, Type::Address { mutable: true, .. } | Type::Desc(_)) {
                writes.push(index as u32);
            }
        }
        let traces = traces(
            module,
            f,
            graph,
            f.parameters[parameter as usize],
            claim.kind,
            &checked,
            budget,
        )?;
        if traces.iter().any(|trace| {
            trace.calls.iter().any(|mode| *mode != claim.kind)
                || claim.kind == CallableKind::FnOnce && trace.calls.len() > 1
        }) {
            return Err(bad(
                f,
                "helper callback use differs from its requested bound or consumes more than once",
            ));
        }
        let expected = CallableParameterContract {
            parameter,
            closure,
            kind: claim.kind,
            hold,
            call,
            writes,
            cleanup: if borrowed {
                CallableCleanup::Borrowed
            } else {
                CallableCleanup::Owned
            },
            traces: CallableTraces::Finite(traces),
        };
        budget.spend(
            claim
                .hold
                .len()
                .saturating_add(claim.call.len())
                .saturating_add(claim.writes.len())
                .saturating_add(claim_traces.len()),
        )?;
        for trace in claim_traces {
            budget.spend(trace.calls.len())?;
        }
        if claim != &expected {
            return Err(bad(
                f,
                "callback Hold/Call/write bound/Cleanup/traces differ from the independent contract",
            ));
        }
        checked[index] = true;
    }
    Ok(())
}
