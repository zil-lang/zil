//! Independent checking of finite concrete callback contracts. The public
//! candidate is compared with a CFG-derived finite language of calls. No
//! producer solver or callee body inlining supplies authority to a caller.
use super::*;
use std::collections::VecDeque;
#[path = "helper_flow.rs"]
mod flow;

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
    parameters: Vec<u32>,
}

/// Independent provenance graph. Edges describe only actual local/SSA
/// transfers; type equality never supplies an edge. The finite marks are
/// roles (1 and 2), verified self-contained factory (4), and other producer (8).
fn environment_origins(
    module: &TypedModule,
    f: &TypedFunction,
    roles: &[CallableRole],
    budget: &mut Budget,
) -> Check<EnvironmentOrigins> {
    let values_at = f.locals.len();
    let count = values_at.saturating_add(f.values.len());
    budget.spend(count.saturating_mul(2))?;
    let mut marks = vec![0u8; count];
    let mut edges = vec![Vec::<usize>::new(); count];
    let mut pending = VecDeque::new();
    budget.spend(roles.len().saturating_mul(2))?;
    let mut parameters = Vec::with_capacity(roles.len());
    for (position, role) in roles.iter().enumerate() {
        let parameter = f.parameters[role.parameter as usize];
        marks[parameter.0 as usize] = 1 << position;
        pending.push_back(parameter.0 as usize);
        parameters.push(role.parameter);
    }
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
            marks[destination] = if factory { 4 } else { 8 };
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
    Ok(EnvironmentOrigins {
        values_at,
        marks,
        parameters,
    })
}

fn invocation(
    f: &TypedFunction,
    instruction: &Instruction,
    origins: &EnvironmentOrigins,
) -> Check<Option<CallEvent>> {
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
    let mark = origins.marks[origin];
    if mark == 4 && !borrowed {
        return Ok(None);
    }
    let parameter = match mark {
        1 => origins.parameters.first().copied(),
        2 => origins.parameters.get(1).copied(),
        _ => None,
    };
    parameter
        .map(|parameter| Some(CallEvent { parameter, kind }))
        .ok_or_else(|| {
            unsupported(
                "helper callable origin is unknown, mixed, or a borrowed local factory result",
                Some(f.span),
            )
        })
}

fn dependency_order(
    module: &TypedModule,
    graphs: &[FunctionGraph],
    limits: &Limits,
    budget: &mut Budget,
) -> Check<Vec<usize>> {
    let count = module.functions.len();
    budget.spend(count.saturating_mul(4))?;
    let mut depths: Option<Vec<usize>> = None;
    let mut predecessors = vec![0usize; count];
    let mut consumers = vec![Vec::new(); count];
    for (caller, f) in module.functions.iter().enumerate() {
        if f.callable_contract.is_none() {
            continue;
        }
        if depths.is_none() {
            budget.spend(count)?;
            depths = Some(vec![1usize; count]);
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
            let depth = depths.as_mut().expect("helper edges have a depth table");
            let next_depth = depth[callee]
                .checked_add(1)
                .filter(|n| *n <= limits.max_call_depth)
                .ok_or_else(|| limit("helper dependency call depth exhausted"))?;
            depth[caller] = depth[caller].max(next_depth);
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
    if depths.is_some() && limits.max_call_depth == 0 {
        return Err(limit("helper dependency call depth exhausted"));
    }
    Ok(order)
}

struct Forwarded<'a> {
    traces: &'a CallableTraces,
    mapping: Vec<(u32, u32)>,
}
impl Forwarded<'_> {
    fn remap(
        &self,
        calls: &[CallEvent],
        budget: &mut Budget,
        f: &TypedFunction,
    ) -> Check<Vec<CallEvent>> {
        budget.spend(
            calls
                .len()
                .saturating_mul(self.mapping.len().saturating_add(1)),
        )?;
        calls
            .iter()
            .map(|event| {
                let parameter = self
                    .mapping
                    .iter()
                    .find(|(child, _)| *child == event.parameter)
                    .map(|(_, caller)| *caller)
                    .ok_or_else(|| bad(f, "child event has no actual role substitution"))?;
                Ok(CallEvent {
                    parameter,
                    kind: event.kind,
                })
            })
            .collect()
    }
}

fn forwarded<'a>(
    module: &'a TypedModule,
    f: &TypedFunction,
    instruction: &Instruction,
    origins: &EnvironmentOrigins,
    roles: &[CallableRole],
    checked: &[bool],
    budget: &mut Budget,
) -> Check<Option<Forwarded<'a>>> {
    let InstructionKind::Call {
        function: callee,
        arguments,
    } = &instruction.kind
    else {
        return Ok(None);
    };
    budget.spend(arguments.len())?;
    let target = &module.functions[callee.0 as usize];
    let Some(claim) = &target.callable_contract else {
        if arguments
            .iter()
            .any(|value| parameter_type(&f.values[value.0 as usize]).is_some())
        {
            return Err(bad(f, "forwarded helper has no joint contract"));
        }
        return Ok(None);
    };
    if !checked[callee.0 as usize] {
        return Err(bad(
            f,
            "nested helper contract was not independently checked",
        ));
    }
    budget.spend(
        claim
            .roles
            .len()
            .saturating_mul(roles.len().saturating_add(1)),
    )?;
    let mut mapping = Vec::with_capacity(claim.roles.len());
    for child in &claim.roles {
        let value = *arguments
            .get(child.parameter as usize)
            .ok_or_else(|| bad(f, "nested helper role parameter is absent"))?;
        let mark = origins.marks[origins.values_at + value.0 as usize];
        let position = match mark {
            1 => 0,
            2 => 1,
            _ => {
                return Err(bad(
                    f,
                    "nested helper must forward an actual unmixed parameter role",
                ));
            }
        };
        let caller = roles
            .get(position)
            .ok_or_else(|| bad(f, "nested helper refers to an absent caller role"))?;
        if child.kind != caller.kind
            || parameter_type(&f.values[value.0 as usize]).map(|(id, _)| id) != Some(child.closure)
        {
            return Err(bad(f, "nested helper actual role type or bound differs"));
        }
        mapping.push((child.parameter, caller.parameter));
    }
    Ok(Some(Forwarded {
        traces: &claim.traces,
        mapping,
    }))
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
    if claim.roles.len() > limits.max_locals {
        return Err(limit("raw callback role allowance exhausted"));
    }
    budget.spend(claim.roles.len())?;
    let mut field_count = 0usize;
    for role in &claim.roles {
        let amount = role
            .hold
            .len()
            .checked_add(role.call.len())
            .and_then(|n| n.checked_add(role.writes.len()))
            .ok_or_else(|| limit("raw callback role metadata overflow"))?;
        field_count = field_count
            .checked_add(amount)
            .filter(|n| *n <= limits.max_nodes)
            .ok_or_else(|| limit("raw callback contract metadata node allowance exhausted"))?;
        budget.spend(amount.saturating_add(2))?;
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

// Independent admission of the new two-type owning boundary. It checks the
// actual parameter locals, both result fields and both capture roots; it does
// not consume a Types/Analyze classification or a claimed callback contract.
fn two_type_state_constructor(
    module: &TypedModule,
    function: &TypedFunction,
    parameters: &[(u32, ClosureId, bool, &Type)],
    budget: &mut Budget,
) -> Check<bool> {
    let [
        (first_position, first, false, _),
        (second_position, second, false, _),
    ] = parameters
    else {
        return Ok(false);
    };
    // No additional work is charged to the already implemented same-type path.
    if first == second {
        return Ok(false);
    }
    let Type::Package { fields, .. } = &function.return_type else {
        return Ok(false);
    };
    resources::charge_payload(&function.return_type, budget)?;
    budget.spend(
        fields
            .len()
            .saturating_add(function.parameters.len())
            .saturating_add(2),
    )?;
    let (mut counts, mut managed) = ([0usize; 2], false);
    for (_, ty) in fields {
        match ty {
            Type::Closure(id) if id == first => counts[0] += 1,
            Type::Closure(id) if id == second => counts[1] += 1,
            Type::Managed(payload)
                if **payload == Type::ByteBuffer
                    || (matches!(payload.as_ref(), Type::Record { .. })
                        && payload.is_copy_value()) =>
            {
                managed = true
            }
            ty if ty.is_copy_value() => {}
            _ => return Ok(false),
        }
    }
    if counts != [1, 1] || !managed {
        return Ok(false);
    }
    for (position, local) in function.parameters.iter().enumerate() {
        if position == *first_position as usize || position == *second_position as usize {
            continue;
        }
        let ty = &function.locals[local.0 as usize].ty;
        let work = closures::type_layout_work(ty, budget)?;
        budget.spend(work)?;
        if !ty.is_copy_value() && !matches!(ty, Type::Managed(_)) {
            return Err(unsupported(
                "two-type owning State constructors require finite Copy or direct Managed ordinary parameters",
                Some(function.span),
            ));
        }
    }
    for id in [*first, *second] {
        budget.spend(1)?;
        let definition = closures::definition(module, id, Some(function.span))?;
        resources::charge_payload(&definition.result, budget)?;
        if !definition.result.is_copy_value() {
            return Err(unsupported(
                "two-type owning State constructors require Copy callback results",
                Some(function.span),
            ));
        }
        budget.spend(1)?;
        closures::self_contained(module, id, budget)?;
    }
    packages::check_type_for(module, &function.return_type, Some(function.span), budget)?;
    Ok(true)
}

// The public by-value FnOnce boundary admits independently checked ordinary
// data inputs and results. Actual invocation operands are checked against the
// concrete entry by the instruction checker, while roles/traces come from CFG.
fn owned_dispatch(
    module: &TypedModule,
    function: &TypedFunction,
    parameters: &[(u32, ClosureId, bool, &Type)],
    claim: &CallableParameterContract,
    budget: &mut Budget,
) -> Check<bool> {
    let [(_, id, false, _)] = parameters else {
        return Ok(false);
    };
    let definition = closures::definition(module, *id, Some(function.span))?;
    if !definition.parameters.iter().any(owned_callbacks::root)
        && !owned_callbacks::root(&definition.result)
        && !owned_callbacks::root(&function.return_type)
    {
        return Ok(false);
    }
    if claim.roles.len() != 1
        || claim.roles[0].kind != CallableKind::FnOnce
        || !definition.lending_captures.is_empty()
    {
        return Err(bad(
            function,
            "owning dispatch requires one by-value FnOnce role with a closed data signature",
        ));
    }
    budget.spend(function.parameters.len().saturating_mul(2))?;
    for (index, local) in function.parameters.iter().enumerate() {
        if index == parameters[0].0 as usize {
            continue;
        }
        let actual = &function.locals[local.0 as usize].ty;
        if owned_callbacks::root(actual) {
            owned_callbacks::value(module, actual, Some(function.span), budget)?;
        } else {
            check_function_signature_type(module, actual, Some(function.span), budget)?;
        }
    }
    if owned_callbacks::root(&function.return_type) {
        owned_callbacks::value(module, &function.return_type, Some(function.span), budget)?;
    } else {
        resources::charge_payload(&function.return_type, budget)?;
        check_type(&function.return_type, Some(function.span))?;
    }
    Ok(true)
}

pub(super) fn check_module(
    module: &TypedModule,
    graphs: &[FunctionGraph],
    limits: &Limits,
    budget: &mut Budget,
) -> Check<()> {
    let order = dependency_order(module, graphs, limits, budget)?;
    budget.spend(module.functions.len())?;
    let mut checked = vec![false; module.functions.len()];
    for index in order {
        let f = &module.functions[index];
        let mut parameters: Vec<(u32, ClosureId, bool, &Type)> = Vec::new();
        let mut owned_union_data = false;
        for (position, local) in f.parameters.iter().enumerate() {
            let ty = &f.locals[local.0 as usize].ty;
            // This constant legacy test piggybacks on the existing parameter
            // pass; it does not introduce a free structural classifier.
            owned_union_data |= errors::owned(ty, false);
            if let Some((closure, borrowed)) = parameter_type(ty) {
                budget.spend(parameters.len())?;
                if parameters
                    .iter()
                    .any(|(position, ..)| f.parameters[*position as usize] == *local)
                {
                    return Err(bad(
                        f,
                        "actual callback parameters cannot share one local slot",
                    ));
                }
                if parameters.len() >= 2 {
                    return Err(unsupported(
                        "a concrete helper supports at most two callback environment parameters",
                        Some(f.span),
                    ));
                }
                budget.spend(1)?;
                parameters.push((position as u32, closure, borrowed, ty));
            }
        }
        if parameters.is_empty() {
            if f.callable_contract.is_some() {
                return Err(bad(
                    f,
                    "non-helper function cannot claim a callback parameter contract",
                ));
            }
            continue;
        }
        if f.foreign.is_some() || f.environment.is_some() {
            return Err(bad(
                f,
                "a callback helper must be an ordinary checked function",
            ));
        }
        // retained::check_claim independently checked the ordinary factory's
        // exact envelope, input self-containment, signature and companions.
        // Reconstruct its one actual owned input here; it is not a synchronous
        // callback helper and must not advertise an empty helper contract.
        if let [(.., input, false, _)] = parameters.as_slice()
            && f.callable_contract.is_none()
            && f.callable_return_contract.is_some()
            && matches!(f.return_type, Type::Closure(result) if result != *input)
        {
            continue;
        }
        // Preserve the single self-contained owning State value boundary.
        if let [(.., closure, false, _)] = parameters.as_slice()
            && f.callable_contract.is_none()
            && matches!(&f.return_type, Type::Package { fields, .. } if fields.iter().any(|(_, ty)| *ty == Type::Closure(*closure)))
        {
            if owned_union_data || budget.sum_function(f.id) {
                budget.spend(f.parameters.len())?;
                for (position, local) in f.parameters.iter().enumerate() {
                    if parameters.iter().any(|(p, ..)| *p as usize == position) {
                        continue;
                    }
                    let ty = &f.locals[local.0 as usize].ty;
                    let sum_data = if matches!(ty, Type::Enum { .. } | Type::ErrorUnion { .. }) {
                        resources::charge_payload(ty, budget)?;
                        sums::owned(ty)
                    } else {
                        sums::needs_shape(ty, budget)?
                    };
                    if sum_data {
                        return Err(unsupported(
                            "ordinary Sum data does not extend owning State constructor parameter permissions",
                            Some(f.span),
                        ));
                    }
                }
            }
            closures::self_contained(module, *closure, budget)?;
            packages::check_type_for(module, &f.return_type, Some(f.span), budget)?;
            continue;
        }
        // A second owned input is a second value, even when both use the same
        // concrete environment type. Reconstruct this finite owning boundary
        // from the actual signature; no producer classification or empty
        // callback contract can stand in for it. Ordinary parameter, move,
        // Package construction and cleanup checks still cover both values.
        if f.callable_contract.is_none()
            && let [
                (first_position, first, false, _),
                (second_position, second, false, _),
            ] = parameters.as_slice()
            && first == second
            && let Type::Package { fields, .. } = &f.return_type
        {
            budget.spend(
                fields
                    .len()
                    .saturating_add(f.parameters.len())
                    .saturating_add(2),
            )?;
            for (position, local) in f.parameters.iter().enumerate() {
                if position == *first_position as usize || position == *second_position as usize {
                    continue;
                }
                let ty = &f.locals[local.0 as usize].ty;
                // The Copy predicate walks a structural type. Pay for that
                // walk before using it to classify an ordinary input.
                let work = closures::type_layout_work(ty, budget)?;
                budget.spend(work)?;
                if !ty.is_copy_value() && !matches!(ty, Type::Managed(_)) {
                    return Err(unsupported(
                        "two-input owning State constructors require finite Copy or direct Managed ordinary parameters",
                        Some(f.span),
                    ));
                }
            }
            let mut callable_fields = 0usize;
            let mut same_environment = true;
            for (_, ty) in fields {
                if let Type::Closure(id) = ty {
                    callable_fields += 1;
                    same_environment &= id == first;
                }
            }
            if same_environment && (1..=2).contains(&callable_fields) {
                closures::self_contained(module, *first, budget)?;
                packages::check_type_for(module, &f.return_type, Some(f.span), budget)?;
                continue;
            }
        }
        if f.callable_contract.is_none()
            && two_type_state_constructor(module, f, &parameters, budget)?
        {
            continue;
        }
        let claim = f.callable_contract.as_ref().ok_or_else(|| {
            bad(
                f,
                "callback helper is missing its independently checkable contract",
            )
        })?;
        if claim.roles.len() != parameters.len() {
            return Err(bad(
                f,
                "callback role table does not exactly cover actual parameters",
            ));
        }
        let owning_dispatch = owned_dispatch(module, f, &parameters, claim, budget)?;
        for (position, local) in f.parameters.iter().enumerate() {
            if owning_dispatch {
                break;
            }
            if parameters.iter().any(|(p, ..)| *p as usize == position) {
                continue;
            }
            let ty = &f.locals[local.0 as usize].ty;
            if !ty.is_copy_value()
                && !matches!(ty, Type::Address { inner, .. } if inner.is_copy_value()
                    || (f.return_type.is_copy_value() && matches!(inner.as_ref(), Type::Slice { element } if element.is_copy_value())))
                && !matches!(ty, Type::Managed(_) | Type::Package { .. })
                && !matches!(ty, Type::Desc(inner) if inner.as_ref() == &Type::ByteBuffer)
            {
                if matches!(ty, Type::Enum { .. } | Type::ErrorUnion { .. }) {
                    budget.mark_sum(Some(f.id.0 as usize))?;
                    sums::check_type_for(module, ty, Some(f.span), budget)?;
                    continue;
                }
                return Err(unsupported(
                    "other callback helper parameters must be finite Copy, one Copy/slice address, Managed, complete Package or ByteBuffer description values; slice arguments require a Copy helper result",
                    Some(f.span),
                ));
            }
        }
        budget.spend(parameters.len())?;
        let mut roles = Vec::with_capacity(parameters.len());
        for ((parameter, closure, borrowed, ty), proposed) in parameters.iter().zip(&claim.roles) {
            if proposed.parameter != *parameter || proposed.closure != *closure {
                return Err(bad(
                    f,
                    "callback roles must match actual parameter order and concrete types",
                ));
            }
            let definition = closures::definition(module, *closure, Some(f.span))?;
            if !owning_dispatch
                && !f.return_type.is_copy_value()
                && !(parameters.len() == 1
                    && *borrowed
                    && closures::lending_result(&f.return_type)
                    && !definition.lending_captures.is_empty())
            {
                return Err(unsupported(
                    "concrete callback helper results require finite Copy or a borrowed environment lending result",
                    Some(f.span),
                ));
            }
            if !closures::supports(definition.kind, proposed.kind) {
                return Err(bad(
                    f,
                    "requested callback bound is not supported by the concrete entry",
                ));
            }
            if !borrowed && proposed.kind == CallableKind::FnMut {
                return Err(unsupported(
                    "this helper slice takes FnMut through an exclusive environment address",
                    Some(f.span),
                ));
            }
            if let Type::Address { mutable, .. } = ty
                && !(proposed.kind == CallableKind::Fn && !mutable
                    || proposed.kind == CallableKind::FnMut && *mutable)
            {
                return Err(bad(
                    f,
                    "borrowed callback parameter permission must match its exact Fn/FnMut entry",
                ));
            }
            let mut hold = Vec::new();
            let mut call = Vec::new();
            let mut writes = Vec::new();
            for (capture, ty) in definition.captures.iter().enumerate() {
                budget.spend(4)?;
                if matches!(ty, Type::Address { .. }) {
                    hold.push(capture as u32);
                }
                if matches!(ty, Type::Address { .. } | Type::Desc(_)) {
                    call.push(capture as u32);
                }
                if matches!(ty, Type::Address { mutable: true, .. } | Type::Desc(_)) {
                    writes.push(capture as u32);
                }
            }
            roles.push(CallableRole {
                parameter: *parameter,
                closure: *closure,
                kind: proposed.kind,
                hold,
                call,
                writes,
                cleanup: if *borrowed {
                    CallableCleanup::Borrowed
                } else {
                    CallableCleanup::Owned
                },
            });
        }
        let traces = flow::reconstruct(module, f, &roles, &checked, limits, budget)?;
        let expected = CallableParameterContract { roles, traces };
        budget.spend(claim.roles.len())?;
        for role in &claim.roles {
            budget.spend(
                role.hold
                    .len()
                    .saturating_add(role.call.len())
                    .saturating_add(role.writes.len()),
            )?;
        }
        flow::comparison_work(&claim.traces, budget)?;
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
