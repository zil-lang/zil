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

/// Each reachable block is processed after all its predecessors. Trace sets
/// merge equal prefixes, instead of enumerating duplicate syntactic paths.
/// Cycles are a reported scope limit, never an empty or truncated contract.
fn traces(
    module: &TypedModule,
    f: &TypedFunction,
    graph: &FunctionGraph,
    parameter: LocalId,
    expected_kind: CallableKind,
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
            if let InstructionKind::Call { arguments, .. } = &instruction.kind {
                budget.spend(arguments.len())?;
                if arguments
                    .iter()
                    .any(|v| parameter_type(&f.values[v.0 as usize]).is_some())
                {
                    return Err(unsupported(
                        "callback helper forwarding and recursive helper contracts are not in this finite slice",
                        Some(instruction.span),
                    ));
                }
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
                    next.insert(count.saturating_add(1));
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
    budget.spend(
        field_count
            .saturating_add(claim.traces.len())
            .saturating_add(1),
    )?;
    if field_count > limits.max_nodes || claim.traces.len() > limits.max_nodes {
        return Err(limit(
            "raw callback contract metadata node allowance exhausted",
        ));
    }
    for trace in &claim.traces {
        budget.spend(trace.calls.len().saturating_add(1))?;
        if trace.calls.len() > limits.max_nodes {
            return Err(limit("raw callback trace node allowance exhausted"));
        }
    }
    Ok(())
}

pub(super) fn check_module(
    module: &TypedModule,
    graphs: &[FunctionGraph],
    budget: &mut Budget,
) -> Check<()> {
    for (f, graph) in module.functions.iter().zip(graphs) {
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
        if !f.return_type.is_copy_value() {
            return Err(unsupported(
                "concrete callback helper results must be finite Copy values",
                Some(f.span),
            ));
        }
        for (index, local) in f.parameters.iter().enumerate() {
            if index as u32 == parameter {
                continue;
            }
            let ty = &f.locals[local.0 as usize].ty;
            if !ty.is_copy_value() && !matches!(ty, Type::Managed(_) | Type::Package { .. }) {
                return Err(unsupported(
                    "other callback helper parameters must be finite Copy, Managed or complete Package values",
                    Some(f.span),
                ));
            }
        }
        let definition = closures::definition(module, closure, Some(f.span))?;
        let Some(claim) = &f.callable_contract else {
            return Err(bad(
                f,
                "callback helper is missing its independently checkable contract",
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
            traces,
        };
        budget.spend(
            claim
                .hold
                .len()
                .saturating_add(claim.call.len())
                .saturating_add(claim.writes.len())
                .saturating_add(claim.traces.len()),
        )?;
        for trace in &claim.traces {
            budget.spend(trace.calls.len())?;
        }
        if claim != &expected {
            return Err(bad(
                f,
                "callback Hold/Call/write bound/Cleanup/traces differ from the independent contract",
            ));
        }
    }
    Ok(())
}
