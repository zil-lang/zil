//! Producer-side finite helper envelopes. A topological path propagation checks
//! claims; independent Verify uses its own provenance and trace reconstruction.
use super::*;
#[path = "helper_flow.rs"]
mod flow;

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

// Actual role provenance is reconstructed here, independently of published
// events. Bits 0/1 identify the first/second actual environment parameter;
// factory and unknown producers have separate bits. A union never chooses one
// role, even when the concrete ClosureId happens to be equal.
const FACTORY: u8 = 4;
const UNKNOWN: u8 = 8;

fn origins(
    module: &TypedModule,
    function: &TypedFunction,
    parameters: &[(u32, LocalId)],
    budget: &mut Budget,
) -> CompileResult<(Vec<u8>, Vec<u8>)> {
    budget.spend(
        function.locals.len().saturating_add(function.values.len()),
        "helper environment origin tables",
    )?;
    let mut positions = vec![0u8; function.locals.len()];
    let mut results = vec![0u8; function.values.len()];
    budget.spend(parameters.len(), "helper role origin seeds")?;
    for (role, (_, local)) in parameters.iter().enumerate() {
        positions[local.0 as usize] = 1 << role;
    }
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
                        FACTORY
                    } else {
                        UNKNOWN
                    }
                }
                _ => UNKNOWN,
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

fn parameter_invocation(origin: u8, borrowed: bool) -> CompileResult<Option<usize>> {
    match (origin, borrowed) {
        (1, _) => Ok(Some(0)),
        (2, _) => Ok(Some(1)),
        (FACTORY, false) => Ok(None),
        _ => Err(vec![Diagnostic::unsupported(
            Phase::Analyze,
            "helper callable origin is unknown, mixed, or a borrowed local factory result",
            None,
        )]),
    }
}

// This dependency order belongs to Analyze. Edges come from reachable direct
// Calls, not from producer-supplied trace rows. Duplicate call sites are real
// charged edges and are removed separately by the topological frontier.
fn dependency_order(module: &TypedModule, budget: &mut Budget) -> CompileResult<Vec<usize>> {
    let count = module.functions.len();
    budget.product(count, 4, 1, "helper dependency tables")?;
    let mut depths: Option<Vec<usize>> = None;
    let mut incoming = vec![0usize; count];
    let mut users = vec![Vec::new(); count];
    for (caller, function) in module.functions.iter().enumerate() {
        if function.callable_contract.is_none() {
            continue;
        }
        if depths.is_none() {
            budget.spend(count, "helper dependency depth table")?;
            depths = Some(vec![1usize; count]);
        }
        budget.spend(
            function.blocks.len().saturating_add(1),
            "helper dependency reachability",
        )?;
        let mut seen = vec![false; function.blocks.len()];
        let mut pending = vec![function.entry.0 as usize];
        while let Some(index) = pending.pop() {
            budget.spend(1, "helper dependency block")?;
            let block = function
                .blocks
                .get(index)
                .ok_or_else(|| bad("helper dependency CFG outside table"))?;
            if seen[index] {
                continue;
            }
            seen[index] = true;
            for instruction in &block.instructions {
                budget.spend(1, "helper dependency instruction")?;
                if let InstructionKind::Call {
                    function: callee, ..
                } = instruction.kind
                {
                    let callee = callee.0 as usize;
                    let target = module
                        .functions
                        .get(callee)
                        .ok_or_else(|| bad("helper dependency target is absent"))?;
                    if target.callable_contract.is_some() {
                        budget.spend(2, "helper dependency edge")?;
                        incoming[caller] += 1;
                        users[callee].push(caller);
                    }
                }
            }
            match block.terminator {
                Terminator::Jump(next) => {
                    budget.spend(1, "helper dependency CFG edge")?;
                    pending.push(next.0 as usize);
                }
                Terminator::Branch {
                    then_block,
                    else_block,
                    ..
                } => {
                    budget.spend(2, "helper dependency CFG edges")?;
                    pending.extend([then_block.0 as usize, else_block.0 as usize]);
                }
                _ => {}
            }
        }
    }
    let mut ready: VecDeque<_> = incoming
        .iter()
        .enumerate()
        .filter_map(|(i, n)| (*n == 0).then_some(i))
        .collect();
    let mut order = Vec::with_capacity(count);
    while let Some(function) = ready.pop_front() {
        budget.spend(
            users[function].len().saturating_add(1),
            "helper dependency frontier",
        )?;
        order.push(function);
        for &user in &users[function] {
            let depth = depths.as_mut().expect("helper edges have a depth table");
            let candidate = depth[function]
                .checked_add(1)
                .filter(|n| *n <= budget.helper_depth)
                .ok_or_else(|| {
                    vec![Diagnostic::limit(
                        Phase::Analyze,
                        "helper dependency call depth exhausted",
                        None,
                    )]
                })?;
            depth[user] = depth[user].max(candidate);
            incoming[user] -= 1;
            if incoming[user] == 0 {
                ready.push_back(user);
            }
        }
    }
    if order.len() != count {
        return Err(vec![Diagnostic::unsupported(
            Phase::Analyze,
            "recursive callable helper dependency requires an explicit finite contract",
            None,
        )]);
    }
    if depths.is_some() && budget.helper_depth == 0 {
        return Err(vec![Diagnostic::limit(
            Phase::Analyze,
            "helper dependency call depth exhausted",
            None,
        )]);
    }
    Ok(order)
}

struct Forwarded<'a> {
    traces: &'a CallableTraces,
    /// Each child parameter is replaced at this actual static call site.
    roles: Vec<(u32, u32)>,
}

fn forwarded<'a>(
    module: &'a TypedModule,
    function: &TypedFunction,
    instruction: &Instruction,
    origins: &[u8],
    roles: &[CallableRole],
    checked: &[bool],
    budget: &mut Budget,
) -> CompileResult<Option<Forwarded<'a>>> {
    let InstructionKind::Call {
        function: callee,
        arguments,
    } = &instruction.kind
    else {
        return Ok(None);
    };
    budget.spend(arguments.len(), "helper forwarded environment scan")?;
    let environments = arguments
        .iter()
        .filter(|value| env(&function.values[value.0 as usize]).is_some())
        .count();
    if environments == 0 {
        return Ok(None);
    }
    let target = module
        .functions
        .get(callee.0 as usize)
        .ok_or_else(|| bad("forwarded helper target is absent"))?;
    let claim = target
        .callable_contract
        .as_ref()
        .ok_or_else(|| bad("forwarded helper has no finite contract"))?;
    if !checked[callee.0 as usize] || claim.roles.len() != environments {
        return Err(bad(
            "nested helper must forward its actual parameter to an independently checked matching contract",
        ));
    }
    budget.spend(claim.roles.len(), "nested helper role substitution table")?;
    let mut substitutions = Vec::with_capacity(claim.roles.len());
    for role in &claim.roles {
        budget.spend(1, "nested helper actual role")?;
        let value = *arguments
            .get(role.parameter as usize)
            .ok_or_else(|| bad("nested helper role argument is absent"))?;
        let origin = parameter_invocation(origins[value.0 as usize], true)?
            .ok_or_else(|| bad("nested helper must forward an actual parent role"))?;
        let parent = roles
            .get(origin)
            .ok_or_else(|| bad("nested helper names an absent parent role"))?;
        if role.kind != parent.kind
            || env(&function.values[value.0 as usize]).map(|(id, _)| id) != Some(role.closure)
        {
            return Err(bad("nested helper role identity or mode differs"));
        }
        substitutions.push((role.parameter, parent.parameter));
    }
    Ok(Some(Forwarded {
        traces: &claim.traces,
        roles: substitutions,
    }))
}

pub(super) fn check(module: &TypedModule, budget: &mut Budget) -> CompileResult<()> {
    let order = dependency_order(module, budget)?;
    budget.spend(module.functions.len(), "checked helper dependencies")?;
    let mut checked = vec![false; module.functions.len()];
    for index in order {
        let function = &module.functions[index];
        // Preserve ordinary functions' established accounting. Their parameter
        // metadata was bounded during preflight and no role table is allocated.
        if function.callable_contract.is_none()
            && !function
                .parameters
                .iter()
                .any(|p| env(&function.locals[p.0 as usize].ty).is_some())
        {
            continue;
        }
        budget.spend(
            function.parameters.len().saturating_add(1),
            "helper parameter envelope and candidate table",
        )?;
        let candidates = function
            .parameters
            .iter()
            .enumerate()
            .filter_map(|(index, p)| {
                env(&function.locals[p.0 as usize].ty).map(|_| (index as u32, *p))
            })
            .collect::<Vec<_>>();
        if function.callable_contract.is_none()
            && candidates.len() == 1
            && function.environment.is_none()
            && function.foreign.is_none()
            && let Type::Closure(id) = function.locals[candidates[0].1.0 as usize].ty
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
        if candidates.is_empty()
            || candidates.len() > 2
            || (candidates.len() == 2 && candidates[0].1 == candidates[1].1)
            || function.environment.is_some()
            || function.foreign.is_some()
        {
            return Err(bad(
                "helper requires one or two concrete environment parameters",
            ));
        }
        budget.spend(claim.roles.len(), "helper exact role table")?;
        if claim.roles.len() != candidates.len()
            || claim
                .roles
                .iter()
                .zip(&candidates)
                .any(|(role, (index, _))| role.parameter != *index)
        {
            return Err(bad(
                "helper roles do not exactly match actual ordered parameters",
            ));
        }
        budget.product(
            function.parameters.len(),
            candidates.len(),
            1,
            "helper extra parameter membership",
        )?;
        if function.parameters.iter().any(|p| {
            !candidates.iter().any(|(_, local)| p == local)
                && !function.locals[p.0 as usize].ty.is_copy_value()
                && !matches!(&function.locals[p.0 as usize].ty,
                    Type::Address { inner, .. } if inner.is_copy_value()
                        || (function.return_type.is_copy_value()
                            && matches!(inner.as_ref(), Type::Slice { element } if element.is_copy_value())))
                && !matches!(&function.locals[p.0 as usize].ty, Type::Desc(inner) if inner.as_ref() == &Type::ByteBuffer)
                && !matches!(
                    function.locals[p.0 as usize].ty,
                    Type::Managed(_) | Type::Package { .. }
                )
        }) {
            return Err(vec![Diagnostic::unsupported(
                Phase::Analyze,
                "helper extra parameters require finite Copy, one Copy/slice address, Managed, Package or ByteBuffer description values; slice arguments require a Copy helper result",
                None,
            )]);
        }
        let mut borrowed_roles = 0u8;
        for (index, (role, (_, local))) in claim.roles.iter().zip(&candidates).enumerate() {
            budget.spend(1, "helper actual role envelope")?;
            let (closure, borrowed) =
                env(&function.locals[local.0 as usize].ty).expect("classified parameter");
            let definition = module
                .closures
                .get(closure.0 as usize)
                .ok_or_else(|| bad("helper environment definition missing"))?;
            if !(function.return_type.is_copy_value()
                || (candidates.len() == 1
                    && borrowed.is_some()
                    && !definition.lending_captures.is_empty()
                    && matches!(&function.return_type, Type::Address { inner, mutable: false }
                        if inner.is_copy_value() || matches!(&**inner, Type::Slice { element } if element.is_copy_value()))))
            {
                return Err(bad(
                    "helper result requires finite Copy or a single shared result from its borrowed environment",
                ));
            }
            if borrowed.is_some() {
                borrowed_roles |= 1 << index;
            }
            if closure != role.closure
                || role.cleanup
                    != if borrowed.is_some() {
                        CallableCleanup::Borrowed
                    } else {
                        CallableCleanup::Owned
                    }
            {
                return Err(bad("helper environment identity or cleanup mode differs"));
            }
            if !matches!(
                (definition.kind, role.kind),
                (
                    CallableKind::Fn,
                    CallableKind::Fn | CallableKind::FnMut | CallableKind::FnOnce
                ) | (
                    CallableKind::FnMut,
                    CallableKind::FnMut | CallableKind::FnOnce
                ) | (CallableKind::FnOnce, CallableKind::FnOnce)
            ) || (role.kind == CallableKind::FnOnce && borrowed.is_some())
                || (role.kind == CallableKind::FnMut && borrowed != Some(true))
            {
                return Err(bad(
                    "helper parameter cannot provide the requested call mode",
                ));
            }
            budget.product(
                definition.captures.len(),
                3,
                1,
                "helper capture envelope buffers",
            )?;
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
            if role.hold != hold || role.call != call || role.writes != writes {
                return Err(bad(
                    "helper Hold/Call/write envelope differs from its concrete captures",
                ));
            }
        }
        flow::check(
            module,
            function,
            claim,
            &candidates,
            borrowed_roles,
            &checked,
            budget,
        )?;
        checked[index] = true;
    }
    Ok(())
}
