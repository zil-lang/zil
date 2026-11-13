//! Independent checking of finite concrete callback contracts. The public
//! candidate is compared with a CFG-derived finite language of calls. No
//! producer solver or callee body inlining supplies authority to a caller.
use super::*;
#[path = "helper_origin.rs"]
mod origin;
use origin::Origin;
use std::collections::VecDeque;
#[path = "helper_capture_bounds.rs"]
mod capture_bounds;
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

fn forwarded_single_slot(f: &TypedFunction, roles: &[CallableRole], ty: &Type) -> bool {
    f.callable_return_contract
        .as_ref()
        .is_some_and(|contract| matches!(contract.source, CallableReturnSource::Parameter(_)))
        && matches!(roles, [role] if matches!(ty,
        Type::Array { element, length: 1 }
            if element.as_ref() == &Type::Closure(role.closure)))
}

pub(super) fn role_data_operand(
    f: &TypedFunction,
    instruction: &Instruction,
    value: ValueId,
) -> bool {
    let Some(claim) = &f.callable_contract else {
        return false;
    };
    let ty = &f.values[value.0 as usize];
    if parameter_type(ty).is_none() && !forwarded_single_slot(f, &claim.roles, ty) {
        return false;
    }
    match &instruction.kind {
        InstructionKind::MakeOwnedArray { .. } => !instruction.result.is_some_and(|result| {
            forwarded_single_slot(f, &claim.roles, &f.values[result.0 as usize])
        }),
        InstructionKind::StoreOwnedArrayElement {
            local, index: 0, ..
        } => !forwarded_single_slot(f, &claim.roles, &f.locals[local.0 as usize].ty),
        InstructionKind::MakePackage { .. }
        | InstructionKind::MakeManaged { .. }
        | InstructionKind::MakeAggregate { .. }
        | InstructionKind::MakeVariant { .. }
        | InstructionKind::MakeErrorUnion { .. }
        | InstructionKind::StorePackageField { .. }
        | InstructionKind::StoreOwnedArrayElement { .. }
        | InstructionKind::UpdateField { .. }
        | InstructionKind::UpdateIndex { .. } => true,
        _ => false,
    }
}

struct EnvironmentOrigins {
    values_at: usize,
    marks: Vec<Origin>,
    parameters: Vec<u32>,
}

impl EnvironmentOrigins {
    fn parameter(&self, mark: Origin) -> Option<u32> {
        mark.index()
            .and_then(|index| self.parameters.get(index).copied())
    }
}
fn local_invocation(mark: Origin, borrowed: bool) -> bool {
    (mark == Origin::FACTORY && !borrowed) || mark == Origin::BODY || mark == Origin::CONCRETE
}

// This is a use restriction, not a closure safety certificate. Only concrete
// constructions that flow to the actual With body through ordinary local
// transfers bypass the legacy nested-helper gate. Resource/loan/capture checks
// still independently prove their initialization, consumption and roots.
fn scoped_local_bodies(f: &TypedFunction, budget: &mut Budget) -> Check<Option<Vec<bool>>> {
    if !budget
        .scoped_functions
        .as_ref()
        .is_some_and(|marks| marks[f.id.0 as usize])
    {
        return Ok(None);
    }
    let values_at = f.locals.len();
    let count = values_at.saturating_add(f.values.len());
    budget.spend(count.saturating_mul(2))?;
    let mut sources = vec![Vec::new(); count];
    let mut bodies = vec![false; count];
    let mut pending = VecDeque::new();
    for block in &f.blocks {
        budget.spend(1)?;
        for instruction in &block.instructions {
            budget.spend(1)?;
            match instruction.kind {
                InstructionKind::WithForeignCallback { body, .. } => {
                    let at = values_at + body.0 as usize;
                    if !bodies[at] {
                        budget.spend(1)?;
                        bodies[at] = true;
                        pending.push_back(at);
                    }
                }
                InstructionKind::Store { local, value } => {
                    budget.spend(1)?;
                    sources[local.0 as usize].push(values_at + value.0 as usize);
                }
                InstructionKind::Load(local) | InstructionKind::TakeLocal { local } => {
                    if let Some(result) = instruction.result {
                        budget.spend(1)?;
                        sources[values_at + result.0 as usize].push(local.0 as usize);
                    }
                }
                _ => {}
            }
        }
    }
    while let Some(at) = pending.pop_front() {
        budget.spend(1)?;
        for &source in &sources[at] {
            budget.spend(1)?;
            if !bodies[source] {
                budget.spend(1)?;
                bodies[source] = true;
                pending.push_back(source);
            }
        }
    }
    Ok(Some(bodies))
}

/// Independent provenance graph. Edges describe only actual local/SSA
/// transfers; type equality never supplies an edge. The finite marks are
/// roles (1 and 2), verified self-contained factory (4), other producer (8),
/// an actual local With body construction (16), and another concrete local
/// construction (32). Both construction classes must preserve capture roles.
fn environment_origins(
    module: &TypedModule,
    f: &TypedFunction,
    role_data_uses: &[ValueId],
    roles: &[CallableRole],
    budget: &mut Budget,
) -> Check<EnvironmentOrigins> {
    let local_bodies = scoped_local_bodies(f, budget)?;
    let values_at = f.locals.len();
    let count = values_at.saturating_add(f.values.len());
    budget.spend(Origin::table_work(count).saturating_add(count))?;
    let mut marks = vec![Origin::EMPTY; count];
    let mut edges = vec![Vec::<usize>::new(); count];
    let mut pending = VecDeque::new();
    budget.spend(roles.len().saturating_mul(2))?;
    let mut parameters = Vec::with_capacity(roles.len());
    let mut concrete_captures = Vec::new();
    for (position, role) in roles.iter().enumerate() {
        let parameter = f.parameters[role.parameter as usize];
        marks[parameter.0 as usize] = Origin::supplied(position, false);
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
            if let InstructionKind::StoreOwnedArrayElement {
                local,
                index: 0,
                value,
            } = instruction.kind
                && forwarded_single_slot(f, roles, &f.locals[local.0 as usize].ty)
            {
                budget.spend(1)?;
                edges[values_at + value.0 as usize].push(local.0 as usize);
            }
            let Some(result) = instruction.result else {
                continue;
            };
            if parameter_type(&f.values[result.0 as usize]).is_none()
                && !forwarded_single_slot(f, roles, &f.values[result.0 as usize])
            {
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
                InstructionKind::TakeOwnedArrayElement { local, index: 0 }
                | InstructionKind::BorrowOwnedArrayElement {
                    local, index: 0, ..
                } if forwarded_single_slot(f, roles, &f.locals[local.0 as usize].ty) => {
                    Some(local.0 as usize)
                }
                InstructionKind::MakeOwnedArray { ref elements }
                    if forwarded_single_slot(f, roles, &f.values[result.0 as usize])
                        && elements.len() == 1 =>
                {
                    Some(values_at + elements[0].0 as usize)
                }
                _ => None,
            };
            if let Some(origin) = origin {
                budget.spend(1)?;
                edges[origin].push(destination);
                continue;
            }
            if let InstructionKind::Call {
                function,
                ref arguments,
            } = instruction.kind
            {
                let producer = &module.functions[function.0 as usize];
                if let Some(contract) = &producer.callable_return_contract
                    && let CallableReturnSource::Parameter(position) = contract.source
                {
                    let value = arguments.get(position as usize).ok_or_else(|| {
                        bad(f, "nested callable result names an absent actual parameter")
                    })?;
                    budget.spend(1)?;
                    edges[values_at + value.0 as usize].push(destination);
                    continue;
                }
            }
            // All return claims and bodies were independently checked by
            // retained before this pass. Do not infer safety from a type ID or
            // from a producer-supplied empty envelope alone.
            let factory = match instruction.kind {
                InstructionKind::Call { function, .. } => {
                    let producer = &module.functions[function.0 as usize];
                    producer.callable_return_contract.as_ref().is_some_and(|c| {
                        c.source == CallableReturnSource::Fresh
                            && f.values[result.0 as usize] == Type::Closure(c.closure)
                            && producer.return_type == Type::Closure(c.closure)
                    })
                }
                _ => false,
            };
            marks[destination] = if factory {
                Origin::FACTORY
            } else if let InstructionKind::MakeClosure { ref captures, .. } = instruction.kind {
                budget.spend(1)?;
                concrete_captures.push(captures);
                if local_bodies
                    .as_ref()
                    .is_some_and(|bodies| bodies[destination])
                {
                    Origin::BODY
                } else {
                    Origin::CONCRETE
                }
            } else {
                Origin::UNKNOWN
            };
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
    for captures in concrete_captures {
        budget.spend(captures.len().saturating_add(1))?;
        for value in captures {
            if parameter_type(&f.values[value.0 as usize]).is_none() {
                continue;
            }
            let origin = marks[values_at + value.0 as usize];
            if !origin.concrete() {
                return Err(bad(
                    f,
                    "local concrete callable captures a supplied, unknown or mixed callable origin",
                ));
            }
        }
    }
    budget.spend(role_data_uses.len())?;
    for value in role_data_uses {
        if !marks[values_at + value.0 as usize].concrete() {
            return Err(bad(
                f,
                "supplied, unknown or mixed callable origin cannot enter aggregate data",
            ));
        }
    }
    Ok(EnvironmentOrigins {
        values_at,
        marks,
        parameters,
    })
}

// Only reconstruct receiver identity here. The full helper pass separately
// supplies every aggregate data use and checks the claimed call traces. This
// worklist is Verify's own transfer graph, not Analyze's role publication.
pub(super) fn moved_receiver_parameter(
    module: &TypedModule,
    f: &TypedFunction,
    local: LocalId,
    budget: &mut Budget,
) -> Check<usize> {
    let claim = f
        .callable_contract
        .as_ref()
        .ok_or_else(|| bad(f, "moved callback has no roles"))?;
    budget.spend(claim.roles.len())?;
    for role in &claim.roles {
        if f.parameters.get(role.parameter as usize).is_none() {
            return Err(bad(f, "moved callback role parameter is absent"));
        }
    }
    let origins = environment_origins(module, f, &[], &claim.roles, budget)?;
    let mark = origins
        .marks
        .get(local.0 as usize)
        .ok_or_else(|| bad(f, "moved callback local is absent"))?;
    origins
        .parameter(*mark)
        .map(|parameter| parameter as usize)
        .ok_or_else(|| {
            bad(
                f,
                "moved callback has unknown or mixed actual parameter origins",
            )
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
    if local_invocation(mark, borrowed) {
        return Ok(None);
    }
    let parameter = origins.parameter(mark);
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
    mapping: Vec<(u32, Option<u32>)>,
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
        let mut result = Vec::new();
        for event in calls {
            let parameter = self
                .mapping
                .iter()
                .find(|(child, _)| *child == event.parameter)
                .map(|(_, caller)| *caller)
                .ok_or_else(|| bad(f, "child event has no actual role substitution"))?;
            if let Some(parameter) = parameter {
                result.push(CallEvent {
                    parameter,
                    kind: event.kind,
                });
            }
        }
        Ok(result)
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
        if mark.concrete() {
            let Some((id, _)) = parameter_type(&f.values[value.0 as usize]) else {
                return Err(bad(
                    f,
                    "child concrete callable lacks its actual environment type",
                ));
            };
            if id != child.closure
                || !closures::supports(
                    closures::definition(module, id, Some(instruction.span))?.kind,
                    child.kind,
                )
            {
                return Err(bad(
                    f,
                    "child concrete callable type or mode differs from its checked role",
                ));
            }
            mapping.push((child.parameter, None));
            continue;
        }
        let position = mark.index().ok_or_else(|| {
            bad(
                f,
                "nested helper must forward an actual unmixed parameter role",
            )
        })?;
        let caller = roles
            .get(position)
            .ok_or_else(|| bad(f, "nested helper refers to an absent caller role"))?;
        if !closures::supports(caller.kind, child.kind)
            || parameter_type(&f.values[value.0 as usize]).map(|(id, _)| id) != Some(child.closure)
        {
            return Err(bad(
                f,
                "nested helper actual role type or bound exceeds the caller declaration",
            ));
        }
        mapping.push((child.parameter, Some(caller.parameter)));
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
        if let Some(life) = &role.life {
            crate::signature_life::charge(life, budget)?;
            budget.borrowed_function_pointers = true;
            budget.invocation_callbacks = true;
        }
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
    let [(_, id, borrowed, _)] = parameters else {
        return Ok(false);
    };
    if claim.roles.len() != 1 {
        return Err(bad(
            function,
            "owned dispatch requires one actual callable role",
        ));
    }
    let definition = closures::definition(module, *id, Some(function.span))?;
    if !definition.parameters.iter().any(owned_callbacks::root)
        && !owned_callbacks::root(&definition.result)
        && !owned_callbacks::root(&function.return_type)
    {
        return Ok(false);
    }
    let borrowed_input_result = if *borrowed || claim.roles[0].kind != CallableKind::FnOnce {
        budget.spend(definition.parameters.len())?;
        resources::charge_payload(&definition.result, budget)?;
        owned_callbacks::root(&definition.result)
            && definition
                .parameters
                .iter()
                .any(|ty| matches!(ty, Type::Address { .. }))
    } else {
        false
    };
    budget.spend(definition.parameters.len().saturating_add(1))?;
    let borrowed_environment = *borrowed
        && matches!(claim.roles[0].kind, CallableKind::Fn | CallableKind::FnMut)
        && !definition
            .parameters
            .iter()
            .any(|ty| matches!(ty, Type::Address { .. }));
    if claim.roles.len() != 1
        || (!borrowed_input_result
            && !borrowed_environment
            && (*borrowed || claim.roles[0].kind != CallableKind::FnOnce))
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
        if matches!(actual, Type::Address { .. }) {
            budget.spend(definition.parameters.len())?;
            resources::charge_payload(&definition.result, budget)?;
            if !(borrowed_environment && actual.is_package_address())
                && (!owned_callbacks::root(&definition.result)
                    || !definition
                        .parameters
                        .iter()
                        .any(|ty| matches!(ty, Type::Address { .. })))
            {
                return Err(bad(
                    function,
                    "borrowed helper companions require a borrowed-input owned-result callback",
                ));
            }
            owned_callbacks::input(module, actual, budget)?;
        } else if owned_callbacks::root(actual) {
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
        if registration::factory(module, f, budget)? {
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
        let mut scoped = budget
            .scoped_functions
            .as_ref()
            .is_some_and(|marks| marks[index]);
        if budget.scoped_functions.is_some() && !scoped {
            // Callees are already checked in dependency order. Propagate only
            // from actual Call edges, never a producer contract classification.
            for block in &f.blocks {
                budget.spend(1)?;
                for instruction in &block.instructions {
                    budget.spend(1)?;
                    if let InstructionKind::Call { function, .. } = instruction.kind {
                        scoped |= budget.scoped_functions.as_ref().unwrap()[function.0 as usize];
                    }
                }
            }
            if scoped {
                budget.spend(1)?;
                budget.scoped_functions.as_mut().unwrap()[index] = true;
            }
        }
        let forwarding = f
            .callable_return_contract
            .as_ref()
            .is_some_and(|contract| matches!(contract.source, CallableReturnSource::Parameter(_)));
        // A Parameter result has its own independently checked signature and
        // instance proof. Old synchronous helper result gates must not turn a
        // returned F into a Copy result or require a FnOnce owning dispatch.
        let owning_dispatch = if forwarding {
            false
        } else {
            owned_dispatch(module, f, &parameters, claim, budget)?
        };
        for (position, local) in f.parameters.iter().enumerate() {
            if owning_dispatch || forwarding {
                break;
            }
            if parameters.iter().any(|(p, ..)| *p as usize == position) {
                continue;
            }
            let ty = &f.locals[local.0 as usize].ty;
            if !ty.is_copy_value()
                && !ty.is_package_address()
                && !(budget.stored_borrows
                    && (matches!(ty, Type::BorrowedStorage { .. })
                        || matches!(ty,Type::Address{inner,..}if matches!(inner.as_ref(),Type::BorrowedStorage{..}))))
                && !matches!(ty, Type::Address { inner, .. } if inner.is_copy_value()
                    || ((f.return_type.is_copy_value() || claim.roles.iter().any(|r|r.life.is_some())) && matches!(inner.as_ref(), Type::Slice { element } if element.is_copy_value())))
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
                && !forwarding
                && !f.return_type.is_copy_value()
                && !(proposed.life.is_some() && matches!(&f.return_type, Type::Address { .. }))
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
            if !borrowed && proposed.kind == CallableKind::FnMut && !forwarding && !scoped {
                return Err(unsupported(
                    "this helper slice takes FnMut through an exclusive environment address",
                    Some(f.span),
                ));
            }
            if let Type::Address { mutable, .. } = ty
                && !(proposed.kind == CallableKind::Fn && !mutable
                    || proposed.kind == CallableKind::FnMut && *mutable
                    || scoped && proposed.kind == CallableKind::Fn && *mutable)
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
                let (held, called, written) = capture_bounds::bounds(module, ty, budget)?;
                if held {
                    hold.push(capture as u32);
                }
                if called {
                    call.push(capture as u32);
                }
                if written {
                    writes.push(capture as u32);
                }
            }
            if let Some(life) = &proposed.life {
                crate::signature_life::charge(life, budget)?;
                borrowed_fp::signature_work(&definition.parameters, &definition.result, budget)?;
                crate::signature_life::role_relation(
                    module,
                    definition,
                    life,
                    Some(f.span),
                    budget,
                )?;
            }
            roles.push(CallableRole {
                life: proposed.life.clone(),
                parameter: *parameter,
                closure: *closure,
                kind: proposed.kind,
                hold,
                call,
                writes,
                cleanup: if forwarding {
                    CallableCleanup::Returned
                } else if *borrowed {
                    CallableCleanup::Borrowed
                } else {
                    CallableCleanup::Owned
                },
            });
        }
        let traces =
            flow::reconstruct(module, f, &graphs[index], &roles, &checked, limits, budget)?;
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

#[cfg(test)]
mod origin_classification_tests {
    use super::*;
    #[test]
    fn legacy_masks_match_independent_event_substitution_classifiers() {
        let origins = EnvironmentOrigins {
            values_at: 0,
            marks: vec![],
            parameters: vec![10, 20],
        };
        for mask in 0..64 {
            let mut mark = Origin::EMPTY;
            for (bit, origin) in [
                (1, Origin::supplied(0, false)),
                (2, Origin::supplied(1, false)),
                (4, Origin::FACTORY),
                (8, Origin::UNKNOWN),
                (16, Origin::BODY),
                (32, Origin::CONCRETE),
            ] {
                if mask & bit != 0 {
                    mark |= origin;
                }
            }
            let expected = match mask {
                1 => Some(10),
                2 => Some(20),
                _ => None,
            };
            assert_eq!(origins.parameter(mark), expected, "mask={mask}");
            assert_eq!(mark.concrete(), matches!(mask, 4 | 16 | 32));
            for borrowed in [false, true] {
                assert_eq!(
                    local_invocation(mark, borrowed),
                    (mask == 4 && !borrowed) || matches!(mask, 16 | 32)
                );
            }
        }
    }
    #[test]
    fn later_actual_parameter_ordinals_remain_separate_from_local_producers() {
        let origins = EnvironmentOrigins {
            values_at: 0,
            marks: vec![],
            parameters: (10..106).collect(),
        };
        for role in 0..origins.parameters.len() {
            let mark = Origin::supplied(role, false);
            assert_eq!(origins.parameter(mark), Some(role as u32 + 10));
            assert!(!local_invocation(mark, false));
            assert_eq!(origins.parameter(mark | Origin::FACTORY), None);
            assert_eq!(
                origins.parameter(mark | Origin::supplied(role + 1, false)),
                None
            );
        }
    }
}
