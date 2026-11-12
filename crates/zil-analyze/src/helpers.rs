//! Producer-side finite helper envelopes. A topological path propagation checks
//! claims; independent Verify uses its own provenance and trace reconstruction.
use super::*;
#[path = "helper_origin.rs"]
mod origin;
pub(super) use origin::Origin;
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

const FACTORY: Origin = Origin::FACTORY;
const UNKNOWN: Origin = Origin::UNKNOWN;

// retained::check_function has independently checked every public factory
// before helpers run. Match this Call's actual result, not just its type ID or
// the presence of a return claim; Parameter forwarding is never Fresh.
fn fresh_result(producer: &TypedFunction, ty: &Type) -> bool {
    producer.callable_return_contract.as_ref().is_some_and(|c| {
        c.source == CallableReturnSource::Fresh
            && *ty == Type::Closure(c.closure)
            && producer.return_type == *ty
    })
}

fn aggregate_operand(kind: &InstructionKind) -> bool {
    matches!(
        kind,
        InstructionKind::MakePackage { .. }
            | InstructionKind::MakeOwnedArray { .. }
            | InstructionKind::MakeManaged { .. }
            | InstructionKind::MakeAggregate { .. }
            | InstructionKind::MakeVariant { .. }
            | InstructionKind::MakeErrorUnion { .. }
            | InstructionKind::StorePackageField { .. }
            | InstructionKind::StoreOwnedArrayElement { .. }
            | InstructionKind::UpdateField { .. }
            | InstructionKind::UpdateIndex { .. }
    )
}

fn origins(
    module: &TypedModule,
    function: &TypedFunction,
    parameters: &[(u32, LocalId)],
    budget: &mut Budget,
) -> CompileResult<(Vec<Origin>, Vec<Origin>)> {
    budget.spend(
        Origin::table_work(function.locals.len().saturating_add(function.values.len())),
        "helper environment origin tables",
    )?;
    let mut positions = vec![Origin::EMPTY; function.locals.len()];
    let mut results = vec![Origin::EMPTY; function.values.len()];
    budget.spend(parameters.len(), "helper role origin seeds")?;
    for (role, (_, local)) in parameters.iter().enumerate() {
        positions[local.0 as usize] = Origin::supplied(
            role,
            env(&function.locals[local.0 as usize].ty).is_some_and(|(_, mode)| mode.is_some()),
        );
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
                | InstructionKind::ReborrowAddress { .. } => Origin::EMPTY,
                InstructionKind::Call {
                    function: callee, ..
                } => {
                    let producer = module
                        .functions
                        .get(callee.0 as usize)
                        .ok_or_else(|| bad("helper factory producer is absent"))?;
                    // retained::check_function has independently checked every
                    // factory before helpers run. Presence alone is not proof.
                    if fresh_result(producer, ty) {
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
                        Origin::EMPTY
                    }
                    _ => Origin::EMPTY,
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

fn parameter_invocation(origin: Origin, borrowed: bool) -> CompileResult<Option<usize>> {
    if let Some(index) = origin.index() {
        return Ok(Some(index));
    }
    if origin == FACTORY && !borrowed {
        return Ok(None);
    }
    Err(vec![Diagnostic::unsupported(
        Phase::Analyze,
        "helper callable origin is unknown, mixed, or a borrowed local factory result",
        None,
    )])
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
    if !budget.parameter_returns.is_empty() || !budget.scoped_functions.is_empty() {
        budget.spend(count, "forward helper dependency flags")?;
        budget.forward_functions = vec![false; count];
        for id in &budget.parameter_returns {
            budget.forward_functions[id.0 as usize] = true;
        }
        budget.spend(
            budget.scoped_functions.len(),
            "scoped actual helper instance proof",
        )?;
        for id in &budget.scoped_functions {
            budget.forward_functions[id.0 as usize] = true;
        }
    }
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
        let mut has_factory_result = false;
        let mut has_composition = false;
        let mut has_aggregate = false;
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
                // Local construction needs actual CFG positions, including
                // captures and later moves; preserve legacy accounting for
                // helpers which contain no local environment construction.
                if matches!(instruction.kind, InstructionKind::MakeClosure { .. }) {
                    if budget.forward_functions.is_empty() {
                        budget.spend(count, "concrete helper dependency flags")?;
                        budget.forward_functions = vec![false; count];
                    }
                    budget.forward_functions[caller] = true;
                }
                // The legacy Copy-only factory-local profile already checks
                // SSA Fn observations and aliases with exact role traces. A
                // Fresh result alone does not replace that contract with the
                // stronger identity proof. New aggregate transfers do need it.
                has_aggregate |= aggregate_operand(&instruction.kind);
                if let InstructionKind::Call {
                    function: callee, ..
                } = instruction.kind
                {
                    let callee = callee.0 as usize;
                    let target = module
                        .functions
                        .get(callee)
                        .ok_or_else(|| bad("helper dependency target is absent"))?;
                    if instruction
                        .result
                        .and_then(|v| function.values.get(v.0 as usize))
                        .is_some_and(|ty| fresh_result(target, ty))
                    {
                        has_factory_result = true;
                    }
                    if target.callable_contract.is_some() {
                        has_composition = true;
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
        // Only new factory/aggregate combinations pay this operand audit.
        // Existing State constructors without a factory keep their old path.
        if has_factory_result
            && has_aggregate
            && !has_composition
            && !budget
                .forward_functions
                .get(caller)
                .copied()
                .unwrap_or(false)
        {
            for (block, reachable) in function.blocks.iter().zip(&seen) {
                budget.spend(1, "factory aggregate composition block")?;
                if !reachable {
                    continue;
                }
                for instruction in &block.instructions {
                    budget.spend(1, "factory aggregate composition instruction")?;
                    if !aggregate_operand(&instruction.kind) {
                        continue;
                    }
                    for value in super::resources::uses(&instruction.kind) {
                        budget.spend(1, "factory composition operand classification")?;
                        has_composition |= function
                            .values
                            .get(value.0 as usize)
                            .is_some_and(|ty| env(ty).is_some());
                    }
                }
            }
        }
        if has_factory_result && has_composition {
            budget.spend(1, "actual public Fresh factory composition proof trigger")?;
            if budget.forward_functions.is_empty() {
                budget.spend(count, "concrete factory helper dependency flags")?;
                budget.forward_functions = vec![false; count];
            }
            budget.forward_functions[caller] = true;
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
            if !budget.forward_functions.is_empty() {
                budget.spend(1, "forward helper dependency propagation")?;
                if budget.forward_functions[function] {
                    budget.forward_functions[user] = true;
                }
            }
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
    roles: Vec<(u32, Option<u32>)>,
}

fn forwarded<'a>(
    module: &'a TypedModule,
    function: &TypedFunction,
    instruction: &Instruction,
    origins: &[Origin],
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
        let actual = origins[value.0 as usize];
        // Only the position-sensitive proof may discharge a concrete local
        // argument. A ClosureId match never supplies a symbolic parent role.
        if actual == FACTORY
            && budget
                .forward_functions
                .get(function.id.0 as usize)
                .copied()
                .unwrap_or(false)
        {
            let (id, _) = env(&function.values[value.0 as usize])
                .ok_or_else(|| bad("nested concrete argument has no environment"))?;
            let definition = module
                .closures
                .get(id.0 as usize)
                .ok_or_else(|| bad("nested concrete environment is absent"))?;
            if role.closure != id || !super::forward::compatible(definition.kind, role.kind) {
                return Err(bad("nested concrete helper identity or mode differs"));
            }
            substitutions.push((role.parameter, None));
            continue;
        }
        let origin = parameter_invocation(actual, true)?
            .ok_or_else(|| bad("nested helper must forward an actual parent role"))?;
        let parent = roles
            .get(origin)
            .ok_or_else(|| bad("nested helper names an absent parent role"))?;
        if !(role.kind == parent.kind
            || (super::forward::parameter(target).is_some()
                && super::forward::compatible(parent.kind, role.kind)))
            || env(&function.values[value.0 as usize]).map(|(id, _)| id) != Some(role.closure)
        {
            return Err(bad("nested helper role identity or mode differs"));
        }
        substitutions.push((role.parameter, Some(parent.parameter)));
    }
    Ok(Some(Forwarded {
        traces: &claim.traces,
        roles: substitutions,
    }))
}

// This is an owning value boundary, not a noescape helper with an empty Call
// claim. Inspect both actual parameter slots and the complete result shape;
// the later resource pass still checks every move, field and cleanup position.
fn two_input_state_constructor(
    module: &TypedModule,
    function: &TypedFunction,
    candidates: &[(u32, LocalId)],
    budget: &mut Budget,
) -> CompileResult<bool> {
    if candidates.len() != 2
        || candidates[0].1 == candidates[1].1
        || function.environment.is_some()
        || function.foreign.is_some()
    {
        return Ok(false);
    }
    let (Type::Closure(first), Type::Closure(second)) = (
        &function.locals[candidates[0].1.0 as usize].ty,
        &function.locals[candidates[1].1.0 as usize].ty,
    ) else {
        return Ok(false);
    };
    if first != second {
        return two_type_state_constructor(module, function, candidates, *first, *second, budget);
    }
    let Type::Package { fields, .. } = &function.return_type else {
        return Ok(false);
    };
    budget.spend(
        fields.len().saturating_add(2),
        "owning State constructor field envelope",
    )?;
    let mut callbacks = 0;
    for (_, field) in fields {
        if let Type::Closure(id) = field {
            if id != first {
                return Ok(false);
            }
            callbacks += 1;
        }
    }
    if !(1..=2).contains(&callbacks) {
        return Ok(false);
    }
    budget.product(
        function.parameters.len(),
        2,
        1,
        "owning State constructor extra parameter membership",
    )?;
    for parameter in &function.parameters {
        if *parameter == candidates[0].1 || *parameter == candidates[1].1 {
            continue;
        }
        let ty = &function.locals[parameter.0 as usize].ty;
        budget.type_work(ty, true, "owning State constructor extra parameter shape")?;
        if !ty.is_copy_value() && !matches!(ty, Type::Managed(_)) {
            return Ok(false);
        }
    }
    // The one shared ClosureId proves a type property only. No parameter or
    // SSA responsibility is deduplicated here or in the ordinary resource pass.
    callable_fields::self_contained(module, *first, budget)?;
    Ok(true)
}

// The new two-type boundary must cover both actual types in distinct fields.
// Merely removing the old equality check would admit a mismatched constructor
// signature without establishing this finite owning result envelope.
fn two_type_state_constructor(
    module: &TypedModule,
    function: &TypedFunction,
    candidates: &[(u32, LocalId)],
    first: ClosureId,
    second: ClosureId,
    budget: &mut Budget,
) -> CompileResult<bool> {
    let Type::Package { fields, .. } = &function.return_type else {
        return Ok(false);
    };
    budget.type_work(
        &function.return_type,
        true,
        "two-type State constructor result",
    )?;
    budget.spend(
        fields.len().saturating_add(2),
        "two-type State field coverage",
    )?;
    let (mut counts, mut managed) = ([0usize; 2], false);
    for (_, ty) in fields {
        match ty {
            Type::Closure(id) if *id == first => counts[0] += 1,
            Type::Closure(id) if *id == second => counts[1] += 1,
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
    budget.product(
        function.parameters.len(),
        2,
        1,
        "two-type State ordinary parameters",
    )?;
    for local in &function.parameters {
        if *local == candidates[0].1 || *local == candidates[1].1 {
            continue;
        }
        let ty = &function.locals[local.0 as usize].ty;
        budget.type_work(ty, true, "two-type State ordinary parameter shape")?;
        if !ty.is_copy_value() && !matches!(ty, Type::Managed(_)) {
            return Ok(false);
        }
    }
    for id in [first, second] {
        budget.spend(1, "two-type State callback definition")?;
        let definition = module
            .closures
            .get(id.0 as usize)
            .ok_or_else(|| bad("two-type State constructor environment is absent"))?;
        if definition.id != id {
            return Err(bad(
                "two-type State constructor environment identity differs",
            ));
        }
        budget.type_work(
            &definition.result,
            true,
            "two-type State Copy callback result",
        )?;
        if !definition.result.is_copy_value() {
            return Err(vec![Diagnostic::unsupported(
                Phase::Analyze,
                "two-type owning State constructors require Copy callback results",
                Some(function.span),
            )]);
        }
        budget.spend(1, "two-type State capture root")?;
        callable_fields::self_contained(module, id, budget)?;
    }
    Ok(true)
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
        if registration::factory(module, function, budget)? {
            checked[index] = true;
            continue;
        }
        budget.spend(
            function.parameters.len().saturating_add(1),
            "helper parameter envelope and candidate table",
        )?;
        let mut candidates = Vec::new();
        let mut sum_state_companion = false;
        // The existing paid envelope visit also records the State exclusion;
        // legacy constructors acquire no second parameter traversal or table.
        for (index, parameter) in function.parameters.iter().enumerate() {
            let ty = &function.locals[parameter.0 as usize].ty;
            if env(ty).is_some() {
                candidates.push((index as u32, *parameter));
            } else {
                let owned_union = matches!(ty,Type::ErrorUnion { error,result }
                    if matches!(error.as_ref(),Type::Package { .. } | Type::Managed(_))
                        || matches!(result.as_ref(),Type::Package { .. } | Type::Managed(_)));
                sum_state_companion |= owned_union || budget.sum_type(ty)?;
            }
        }
        // retained::check_function already reconstructed every public factory
        // from its actual signature, capture graph and reachable Return sites.
        // One direct owning input is an ordinary transfer, not a noescape role.
        if function.callable_contract.is_none()
            && function.callable_return_contract.is_some()
            && candidates.len() == 1
            && matches!(
                function.locals[candidates[0].1.0 as usize].ty,
                Type::Closure(_)
            )
            && matches!(function.return_type, Type::Closure(_))
        {
            continue;
        }
        if function.callable_contract.is_none()
            && candidates.len() == 1
            && function.environment.is_none()
            && function.foreign.is_none()
            && let Type::Closure(id) = function.locals[candidates[0].1.0 as usize].ty
            && matches!(&function.return_type,Type::Package{fields,..}
                if fields.iter().any(|(_,ty)| *ty==Type::Closure(id)))
        {
            if sum_state_companion {
                return Err(vec![Diagnostic::unsupported(
                    Phase::Analyze,
                    "owning State constructor data parameters do not acquire ordinary Sum qualification",
                    Some(function.span),
                )]);
            }
            callable_fields::self_contained(module, id, budget)?;
            continue;
        }
        if function.callable_contract.is_none()
            && candidates.len() == 2
            && two_input_state_constructor(module, function, &candidates, budget)?
        {
            continue;
        }
        let claim = function
            .callable_contract
            .as_ref()
            .ok_or_else(|| bad("callable parameter has no public helper contract"))?;
        if candidates.len() > 2 {
            budget.product(
                candidates.len(),
                candidates.len(),
                1,
                "helper actual role alias comparisons",
            )?;
        }
        if candidates.is_empty()
            || candidates
                .iter()
                .enumerate()
                .any(|(i, (_, local))| candidates[..i].iter().any(|(_, prior)| prior == local))
            || function.environment.is_some()
            || function.foreign.is_some()
        {
            return Err(bad("helper requires concrete environment parameters"));
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
        // Ordinary data parameters were admitted by bounded module preflight.
        // Their callee CFG still conserves each actual resource; this does not
        // change any callback role, invocation signature or helper result.
        if function.parameters.iter().any(|p| {
            !candidates.iter().any(|(_, local)| p == local)
                && !(budget.stored_borrows && (matches!(&function.locals[p.0 as usize].ty,Type::BorrowedStorage{..}) || matches!(&function.locals[p.0 as usize].ty,Type::Address{inner,..}if matches!(inner.as_ref(),Type::BorrowedStorage{..}))))
                && !function.locals[p.0 as usize].ty.is_copy_value()
                && !function.locals[p.0 as usize].ty.is_package_address()
                && !matches!(&function.locals[p.0 as usize].ty,
                    Type::Address { inner, .. } if inner.is_copy_value()
                        || ((function.return_type.is_copy_value()
                            || matches!(function.return_type, Type::Package { .. } | Type::Enum { .. } | Type::ErrorUnion { .. })
                            || claim.roles.iter().any(|r|r.life.is_some()))
                            && matches!(inner.as_ref(), Type::Slice { element } if element.is_copy_value())))
                && !matches!(&function.locals[p.0 as usize].ty, Type::Desc(inner) if inner.as_ref() == &Type::ByteBuffer)
                && !matches!(
                    function.locals[p.0 as usize].ty,
                    Type::Managed(_) | Type::Package { .. } | Type::Enum { .. } | Type::ErrorUnion { .. }
                )
        }) {
            return Err(vec![Diagnostic::unsupported(
                Phase::Analyze,
                "helper extra parameters require finite Copy, one Copy/slice address, Managed, Package, ordinary Enum/ErrorUnion or ByteBuffer description values; slice arguments require a Copy helper result",
                None,
            )]);
        }
        for (role, (_, local)) in claim.roles.iter().zip(&candidates) {
            budget.spend(1, "helper actual role envelope")?;
            let (closure, borrowed) =
                env(&function.locals[local.0 as usize].ty).expect("classified parameter");
            let definition = module
                .closures
                .get(closure.0 as usize)
                .ok_or_else(|| bad("helper environment definition missing"))?;
            let returned = super::forward::parameter(function).is_some();
            let owned_data = budget.owned_callbacks.contains(&closure)
                || (!function.return_type.is_copy_value()
                    && !matches!(function.return_type, Type::Address { .. }));
            if returned {
                budget.lookup(
                    budget.owned_callback_helpers.len(),
                    "forward helper signature inventory",
                )?;
                budget.owned_callback_helpers.insert(function.id);
            } else if owned_data {
                let borrowed_input_result =
                    if borrowed.is_some() || role.kind != CallableKind::FnOnce {
                        budget.spend(
                            definition.parameters.len(),
                            "borrowed-input owning helper signature",
                        )?;
                        budget.type_work(
                            &definition.result,
                            true,
                            "borrowed-input owning helper result",
                        )?;
                        matches!(
                            definition.result,
                            Type::Package { .. } | Type::Enum { .. } | Type::ErrorUnion { .. }
                        ) && !definition.result.is_copy_value()
                            && definition
                                .parameters
                                .iter()
                                .any(|ty| matches!(ty, Type::Address { .. }))
                    } else {
                        false
                    };
                // Borrowing the callable does not borrow its independently owned data operands.
                // Actual role access, operand consumption and every CFG exit remain checked.
                budget.spend(
                    definition.parameters.len(),
                    "borrowed environment data signature",
                )?;
                let borrowed_environment = borrowed.is_some()
                    && matches!(role.kind, CallableKind::Fn | CallableKind::FnMut)
                    && !definition
                        .parameters
                        .iter()
                        .any(|ty| matches!(ty, Type::Address { .. }));
                if candidates.len() != 1
                    || (!borrowed_input_result
                        && !borrowed_environment
                        && (borrowed.is_some() || role.kind != CallableKind::FnOnce))
                    || function.result_mode != PassingMode::Infer
                {
                    return Err(bad(
                        "owned callback helper requires one by-value FnOnce role",
                    ));
                }
                budget.lookup(
                    budget.owned_callback_helpers.len(),
                    "owned callback helper inventory",
                )?;
                budget.owned_callback_helpers.insert(function.id);
                super::owned_callbacks::data(module, &function.return_type, budget)?;
                budget.spend(
                    function.parameters.len(),
                    "owned callback helper data positions",
                )?;
                for parameter in &function.parameters {
                    if parameter != local {
                        let ty = &function.locals[parameter.0 as usize].ty;
                        if matches!(ty, Type::Address { .. }) {
                            budget
                                .spend(definition.parameters.len(), "owned helper input profile")?;
                            budget.type_work(
                                &definition.result,
                                true,
                                "owned helper input result profile",
                            )?;
                            if !(borrowed_environment && ty.is_package_address())
                                && (!matches!(
                                    definition.result,
                                    Type::Package { .. }
                                        | Type::Enum { .. }
                                        | Type::ErrorUnion { .. }
                                ) || definition.result.is_copy_value()
                                    || !definition
                                        .parameters
                                        .iter()
                                        .any(|ty| matches!(ty, Type::Address { .. })))
                            {
                                return Err(bad(
                                    "borrowed helper companions require a borrowed-input owned-result callback",
                                ));
                            }
                            super::owned_callbacks::input(module, ty, budget)?;
                        } else {
                            super::owned_callbacks::data(module, ty, budget)?;
                        }
                    }
                }
            }
            if let Some(life) = &role.life {
                crate::signature_life::charge(life, budget)?;
                super::borrowed_fp::signature_work(
                    &definition.parameters,
                    &definition.result,
                    budget,
                )?;
                if let Some(actual) = &definition.storage_life {
                    crate::signature_life::charge(actual, budget)?;
                    if actual != life {
                        return Err(vec![Diagnostic::unsupported(
                            Phase::Analyze,
                            "Fixed helper role needs the complete actual capture relation",
                            None,
                        )]);
                    }
                    super::borrowed_fp::qualified_array_relation_work(
                        &definition.captures,
                        &Type::Unit,
                        budget,
                    )?;
                    super::signature_life::closure_relation(
                        &module.closures,
                        definition.kind,
                        &definition.parameters,
                        &definition.captures,
                        super::stored_callables::life_result(definition),
                        life,
                    )?;
                    budget.spend(
                        function.parameters.len(),
                        "captured Fixed helper shared arguments",
                    )?;
                    if claim.roles.len() > 1 {
                        budget.spend(
                            function.parameters.len().saturating_mul(claim.roles.len()),
                            "actual exclusive callable role exclusion",
                        )?;
                    }
                    if function.parameters.iter().enumerate().any(|(position,p)| {
                        p != local
                            && matches!(function.locals[p.0 as usize].ty,Type::Address { mutable:true,.. })
                            && !claim.roles.iter().any(|other|other.parameter as usize==position && other.kind==CallableKind::FnMut
                                && matches!(&function.locals[p.0 as usize].ty,Type::Address {mutable:true,inner} if inner.as_ref()==&Type::Closure(other.closure)))
                    }) {
                        return Err(vec![Diagnostic::unsupported(
                            Phase::Analyze,
                            "captured Fixed helper exclusive arguments need capture target summary substitution",
                            None,
                        )]);
                    }
                } else {
                    super::borrowed_fp::relation(&definition.parameters, &definition.result, life)?;
                }
            }
            if !(returned
                || (role.life.is_some() && matches!(&function.return_type, Type::Address { .. }))
                || owned_data
                || function.return_type.is_copy_value()
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
            if closure != role.closure
                || role.cleanup
                    != if returned {
                        CallableCleanup::Returned
                    } else if borrowed.is_some() {
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
                || (role.kind == CallableKind::FnMut
                    && borrowed != Some(true)
                    && !(returned && borrowed.is_none()))
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
                if matches!(capture, Type::BorrowedStorage { .. } | Type::Closure(_))
                    && budget.returned_borrowed_environments.contains(&closure)
                {
                    let (held, called, written) =
                        super::retained::capture_effects(module, capture, budget)?;
                    if held {
                        hold.push(i as u32);
                    }
                    if called {
                        call.push(i as u32);
                    }
                    if written {
                        writes.push(i as u32);
                    }
                    continue;
                }
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
        flow::check(module, function, claim, &candidates, &checked, budget)?;
        checked[index] = true;
    }
    Ok(())
}

#[cfg(test)]
mod origin_classification_tests {
    use super::*;
    #[test]
    fn legacy_masks_match_actual_invocation_and_borrowed_drop_classifiers() {
        for borrowed_roles in 0..4 {
            for mask in 0..16 {
                let mut origin = Origin::EMPTY;
                for (bit, mark) in [
                    (1, Origin::supplied(0, borrowed_roles & 1 != 0)),
                    (2, Origin::supplied(1, borrowed_roles & 2 != 0)),
                    (4, FACTORY),
                    (8, UNKNOWN),
                ] {
                    if mask & bit != 0 {
                        origin |= mark;
                    }
                }
                assert_eq!(origin.has_role(), mask & 3 != 0);
                assert_eq!(origin.has_borrowed(), mask & borrowed_roles != 0);
                for borrowed in [false, true] {
                    let expected = match mask {
                        1 => Some(Some(0)),
                        2 => Some(Some(1)),
                        4 if !borrowed => Some(None),
                        _ => None,
                    };
                    assert_eq!(
                        parameter_invocation(origin, borrowed).ok(),
                        expected,
                        "mask={mask} borrowed={borrowed} modes={borrowed_roles}"
                    );
                }
            }
        }
    }
    #[test]
    fn later_roles_use_exact_ordinals_and_keep_borrowed_participation() {
        for role in 0..96 {
            for borrowed in [false, true] {
                let origin = Origin::supplied(role, borrowed);
                assert_eq!(parameter_invocation(origin, false).unwrap(), Some(role));
                assert_eq!(origin.has_borrowed(), borrowed);
                assert!(parameter_invocation(origin | FACTORY, false).is_err());
                let mixed = origin | Origin::supplied(role + 1, false);
                assert!(parameter_invocation(mixed, false).is_err());
                assert_eq!(mixed.has_borrowed(), borrowed);
            }
        }
    }
}
