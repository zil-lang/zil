//! Opaque callable results are untrusted claims until the concrete environment,
//! its public boundary and every ordinary return have been checked here.
use super::*;
mod expected_life;
#[path = "retained/provenance.rs"]
mod provenance;
#[path = "retained/sources.rs"]
pub(super) mod sources;

fn bad(f: &TypedFunction, message: impl Into<String>) -> Diagnostic {
    reject("E-VERIFY-CALLABLE-CONTRACT", Some(f.span), message)
}

// Only the new owning-input boundary needs this narrower signature. Existing
// ordinary factories retain their lending profile and their exact old work.
fn owning_signature(
    module: &TypedModule,
    f: &TypedFunction,
    id: ClosureId,
    budget: &mut Budget,
) -> Check<()> {
    let definition = closures::definition(module, id, Some(f.span))?;
    budget.spend(definition.parameters.len().saturating_add(1))?;
    for ty in definition
        .parameters
        .iter()
        .chain(std::iter::once(&definition.result))
    {
        let work = closures::type_layout_work(ty, budget)?;
        budget.spend(work)?;
    }
    if !definition.result.is_copy_value()
        || definition.parameters.iter().any(|ty| {
            !ty.is_copy_value()
                && !matches!(ty, Type::Address { inner, .. }
                    if inner.is_copy_value()
                        || matches!(inner.as_ref(), Type::Slice { element } if element.is_copy_value()))
        })
    {
        return Err(unsupported(
            "owning callable factories require finite Copy results and Copy or synchronous Copy/slice-address parameters",
            Some(f.span),
        ));
    }
    Ok(())
}

/// Runs before fingerprinting. Raw callers cannot hide huge index vectors in a
/// forged contract and force unbounded hashing or equality work.
pub(super) fn check_claim(
    module: &TypedModule,
    f: &TypedFunction,
    limits: &Limits,
    budget: &mut Budget,
) -> Check<()> {
    budget.spend(1)?;
    let constant_result = f
        .environment
        .and_then(|id| module.closures.get(id.0 as usize))
        .is_some_and(super::stored_callables::static_result);
    if constant_result && f.retained_return_sources.is_none() {
        return Err(bad(f, "constant result source upper is Unknown"));
    }
    if let Some(bindings) = &f.retained_return_sources {
        if constant_result {
            if bindings.len() != 1
                || bindings[0].position.base != StorageBase::Result
                || !bindings[0].position.path.is_empty()
                || bindings[0].sources.len() != 1
            {
                return Err(bad(
                    f,
                    "constant result requires its complete canonical source upper",
                ));
            }
            match &bindings[0].sources[0] {
                StorageSource::Static { leaf, mode } => {
                    super::dynamic_metadata::ty(leaf, budget)?;
                    if *mode != StoredBorrowMode::SharedAddress
                        || leaf != &f.return_type
                        || !matches!(leaf, Type::Address { mutable: false, .. })
                    {
                        return Err(bad(
                            f,
                            "constant result upper changes the actual full leaf or mode",
                        ));
                    }
                }
                _ => {
                    return Err(bad(
                        f,
                        "constant result lacks its explicit Static source upper",
                    ));
                }
            }
        }
        if bindings.len() > limits.max_nodes {
            return Err(limit("retained return binding count exceeds node bound"));
        }
        budget.spend(bindings.len())?;
        for binding in bindings {
            if binding.sources.len() > limits.max_nodes
                || binding.position.path.len() > limits.max_call_depth
            {
                return Err(limit("retained return source relation exceeds bounds"));
            }
            budget.spend(
                binding
                    .position
                    .path
                    .len()
                    .saturating_add(binding.sources.len())
                    .saturating_add(1),
            )?;
            for atom in &binding.sources {
                if constant_result {
                    continue;
                }
                if let StorageSource::Exact(source) = atom {
                    // Preserve the established position-only fee. The vector
                    // charge above already accounts for this atom's tag.
                    if source.path.len() > limits.max_call_depth {
                        return Err(limit("retained return source path exceeds depth"));
                    }
                    budget.spend(source.path.len().saturating_add(1))?;
                } else {
                    // Metadata traversal bounds every nested Type and selector;
                    // only the independent memory kernel can prove this source.
                    dynamic_metadata::source(atom, budget)?;
                }
            }
        }
        if !constant_result && !budget.dyn_contexts {
            return Err(bad(
                f,
                "explicit retained return sources lack complete retained result proof",
            ));
        }
        budget.spend(1)?;
        if !constant_result {
            budget.pending_dyn_returns.insert(f.id);
        }
    }
    if let Some(c) = &f.callable_return_contract {
        if let Some(life) = &c.life {
            let work = storage_relation_metadata_work(
                life,
                limits.max_nodes,
                limits.max_call_depth,
                budget.remaining,
            )
            .ok_or_else(|| limit("callable return relation exceeds bounds"))?;
            budget.spend(work)?;
        }
        budget.spend(1)?;
        budget.callable_source_functions.push(f.id);
        let bindings = c
            .capture_sources
            .as_ref()
            .ok_or_else(|| bad(f, "callable capture sources are unknown"))?;
        if bindings.len() > limits.max_nodes {
            return Err(limit("callable capture source count exceeds node bound"));
        }
        budget.spend(bindings.len().saturating_add(1))?;
        for binding in bindings {
            if binding.sources.len() > limits.max_nodes
                || binding.position.path.len() > limits.max_call_depth
            {
                return Err(limit("callable capture source relation exceeds bounds"));
            }
            budget.spend(
                binding
                    .sources
                    .len()
                    .saturating_add(binding.position.path.len())
                    .saturating_add(1),
            )?;
            for atom in &binding.sources {
                if let StorageSource::Static { leaf, .. } = atom {
                    // Prehash metadata work only. The leaf shape, actual source
                    // and complete constructor/write invariant are separate proofs.
                    budget.spend(2)?;
                    crate::dynamic_metadata::ty(leaf, budget)?;
                } else {
                    let source = crate::exact_storage_source(atom)?;
                    if source.path.len() > limits.max_call_depth {
                        return Err(limit("callable source path exceeds depth bound"));
                    }
                    budget.spend(source.path.len().saturating_add(1))?;
                }
            }
        }

        // This public source is read here and by module fingerprinting. A
        // Parameter source also carries its checked index in both walks.
        budget.spend(match c.source {
            CallableReturnSource::Fresh => 2,
            CallableReturnSource::Parameter(_) => 4,
        })?;
        for indices in [&c.hold, &c.call, &c.writes, &c.cleanup, &c.lending_captures] {
            if indices.len() > limits.max_locals {
                return Err(limit("callable result index vector exceeds local bound"));
            }
            budget.spend(indices.len().saturating_add(1))?;
        }
    }
    let Type::Closure(id) = f.return_type else {
        return if f.callable_return_contract.is_none() {
            Ok(())
        } else {
            Err(bad(
                f,
                "a non-closure result cannot claim a callable return contract",
            ))
        };
    };
    let c = f.callable_return_contract.as_ref().ok_or_else(|| {
        bad(
            f,
            "a callable result requires its independently checkable return contract",
        )
    })?;
    let context_result =
        budget.dyn_contexts && dynamic_ports::has_context(module, &f.return_type, budget)?;
    if context_result && c.life.is_none() {
        return Err(bad(
            f,
            "Context callable result lacks its complete expected life relation",
        ));
    }
    if c.life.is_some() {
        let actual = closures::definition(module, id, Some(f.span))?;
        expected_life::check(f, c, actual, budget)?;
    }
    if let CallableReturnSource::Parameter(parameter) = c.source {
        return check_parameter_claim(module, f, id, parameter, c, budget);
    }
    if f.foreign.is_some()
        || f.environment.is_some()
        || f.callable_contract.is_some()
        || f.result_mode != PassingMode::Infer
    {
        return Err(bad(
            f,
            "a callable factory must be an ordinary function with Value result mode",
        ));
    }
    let d = closures::definition(module, id, Some(f.span))?;
    let external_leaves = sources::capture_leaves(module, id, limits, budget)?;
    if !external_leaves.is_empty() || c.capture_sources.as_ref().is_some_and(|s| !s.is_empty()) {
        budget.returned_environment_sources = true;
        budget.spend(external_leaves.len())?;
        budget.nested_callable_sources |= external_leaves
            .iter()
            .any(|leaf| !leaf.position.path.is_empty());
        owned_callbacks::signature(module, d, budget)?;
        return sources::envelope(module, f, id, &external_leaves, budget);
    }
    // The concrete entry signature is independently checked by check_definitions.
    // Returning its self-contained environment does not narrow its data slots.
    owned_callbacks::signature(module, d, budget)?;
    if c.closure != id || c.kind != d.kind {
        return Err(bad(
            f,
            "returned environment identity or exact callable kind disagrees",
        ));
    }
    if c.lending_captures != d.lending_captures {
        return Err(bad(
            f,
            "opaque lending sources differ from the concrete environment definition",
        ));
    }
    if !c.hold.is_empty() || !c.call.is_empty() || !c.writes.is_empty() {
        return Err(bad(
            f,
            "self-contained factory results cannot advertise external dependencies",
        ));
    }
    // A complete physically Copy environment cannot retain an input address.
    // Its logical classes still travel through the full P/Result/Capture
    // relation; no class is erased merely because there are no data leaves.
    // The actual constructor/returns and complete class flow remain mandatory.
    let mut qualified_copy_environment = f.storage_life.is_some()
        && c.life.is_some()
        && d.storage_life.is_some()
        && d.captures.iter().any(|ty| matches!(ty, Type::BorrowedStorage { .. }));
    if qualified_copy_environment {
        budget.spend(d.captures.len())?;
        for ty in &d.captures {
            if !(stored_callables::scalar(ty)
                || stored_callables::inline_qualified_capture(module, ty, budget)?)
            {
                qualified_copy_environment = false;
                break;
            }
        }
    }
    // Every other empty external envelope keeps its existing input proof.
    // A header or a producer's restriction alone grants no authority here.
    budget.spend(f.parameters.len())?;
    let mut owned_input = None;
    for parameter in &f.parameters {
        let local = f
            .locals
            .get(parameter.0 as usize)
            .ok_or_else(|| bad(f, "callable factory parameter is outside its local table"))?;
        if let Type::Closure(input) = local.ty {
            if owned_input.replace(input).is_some() || input == id {
                return Err(bad(
                    f,
                    "an owning callable factory requires one input environment and a distinct grounded result identity",
                ));
            }
            closures::self_contained(module, input, budget)?;
            owning_signature(module, f, input, budget)?;
        } else if let Type::OwnedDyn { interface } = local.ty {
            dynamic_dispatch::type_owned(module, interface, budget)?;
        } else if matches!(local.ty, Type::Package { .. }) {
            packages::check_ordinary_type(module, &local.ty, Some(local.span), budget)?;
        } else if budget.inline_store_root(&local.ty)?
            && dynamic_store::inline_sum_capture(&local.ty, budget)?
        {
            dynamic_store::check_owned_capture(module, &local.ty, Some(limits), budget)?;
        } else if qualified_copy_environment
            && stored_callables::shared_input(&local.ty)
        {
            // Shared input borrowing is independently checked at this actual
            // ABI, caller and CFG. The complete qualified environment above
            // contains no physical pointer that could escape that scope.
        } else if !(local.ty.is_copy_value() || matches!(local.ty, Type::Managed(_))) {
            return Err(unsupported(
                "self-contained callable factories require Copy, Managed or finite Package inputs",
                Some(local.span),
            ));
        }
    }
    if owned_input.is_some() {
        owning_signature(module, f, id, budget)?;
        // Ordinary Package admission also includes existing State and owned
        // array profiles. The new factory only permits capturable companions;
        // a second callback input cannot be hidden in a State shell.
        budget.spend(f.parameters.len())?;
        for parameter in &f.parameters {
            let local = &f.locals[parameter.0 as usize];
            if dynamic_store::is_carrier(&local.ty)
                || (budget.inline_store_root(&local.ty)?
                    && dynamic_store::inline_sum_capture(&local.ty, budget)?)
            {
                dynamic_store::check_owned_capture(module, &local.ty, Some(limits), budget)?;
            } else if matches!(local.ty, Type::Package { .. }) {
                let work = closures::type_layout_work(&local.ty, budget)?;
                budget.spend(work)?;
                if packages::has_callable_fields(&local.ty) || packages::has_array_fields(&local.ty)
                {
                    return Err(unsupported(
                        "owning callable factory companions cannot hide State callbacks or owned arrays",
                        Some(local.span),
                    ));
                }
                if packages::has_store_fields(&local.ty)
                    || (budget.inline_store_root(&local.ty)?
                        && dynamic_store::owned_capture_store(&local.ty, budget)?)
                {
                    dynamic_store::check_owned_capture(module, &local.ty, Some(limits), budget)?;
                }
                packages::check_capture_type(module, &local.ty, Some(local.span), budget)?;
            }
        }
    }
    let mut cleanup = Vec::new();
    for (index, ty) in d.captures.iter().enumerate() {
        budget.spend(1)?;
        if matches!(ty, Type::Package { .. }) {
            packages::check_capture_type(module, ty, Some(f.span), budget)?;
            cleanup.push(index as u32);
        } else if let Type::OwnedDyn { interface } = ty {
            dynamic_dispatch::closed_owned(module, *interface, budget)?;
            cleanup.push(index as u32);
        } else if matches!(ty, Type::Managed(_)) {
            cleanup.push(index as u32);
        } else if budget.inline_store_root(ty)? && dynamic_store::inline_sum_capture(ty, budget)? {
            dynamic_store::check_owned_capture(module, ty, Some(limits), budget)?;
            cleanup.push(index as u32);
        } else if let Type::Closure(id) = ty {
            closures::self_contained(module, *id, budget)?;
            cleanup.push(index as u32);
        } else if !ty.is_copy_value()
            && !stored_callables::inline_qualified_capture(module, ty, budget)?
        {
            return Err(unsupported(
                "returned callable captures must be self-contained finite Copy, Managed or Package values",
                Some(f.span),
            ));
        }
    }
    if c.cleanup != cleanup {
        return Err(bad(
            f,
            "callable result cleanup must name every owned capture exactly in field order",
        ));
    }
    Ok(())
}

/// The public relation names one actual by-value parameter, not a closure
/// type. The CFG proof below must still establish that every return carries
/// this invocation's particular value and its remaining responsibility.
fn check_parameter_claim(
    module: &TypedModule,
    f: &TypedFunction,
    id: ClosureId,
    parameter: u32,
    claim: &CallableReturnContract,
    budget: &mut Budget,
) -> Check<()> {
    if f.foreign.is_some() || f.environment.is_some() || f.result_mode != PassingMode::Infer {
        return Err(bad(
            f,
            "callable forwarding requires an ordinary Value function",
        ));
    }
    let index = parameter as usize;
    let local = f
        .parameters
        .get(index)
        .and_then(|id| f.locals.get(id.0 as usize))
        .ok_or_else(|| {
            bad(
                f,
                "forwarded parameter index is outside the actual signature",
            )
        })?;
    if local.ty != Type::Closure(id)
        || f.parameters
            .iter()
            .filter(|local| {
                matches!(
                    f.locals.get(local.0 as usize).map(|l| &l.ty),
                    Some(Type::Closure(_))
                )
            })
            .count()
            != 1
    {
        return Err(bad(
            f,
            "forwarding requires exactly one actual by-value callable parameter",
        ));
    }
    let roles = &f
        .callable_contract
        .as_ref()
        .ok_or_else(|| {
            bad(
                f,
                "forwarding requires an independently checked callable role",
            )
        })?
        .roles;
    let [role] = roles.as_slice() else {
        return Err(bad(f, "forwarding requires exactly one callable role"));
    };
    let definition = closures::definition(module, id, Some(f.span))?;
    if role.parameter != parameter
        || role.closure != id
        || role.cleanup != CallableCleanup::Returned
        || claim.closure != id
        || claim.kind != role.kind
        || !closures::supports(definition.kind, role.kind)
        || claim.lending_captures != definition.lending_captures
    {
        return Err(bad(
            f,
            "forwarded source, declared mode or self-contained envelope differs from the actual role",
        ));
    }
    // This branch checks a complete external envelope separately from the
    // path-sensitive same-instance proof in check_body. Choosing the legacy
    // empty path never hides a borrow: self_contained recursively rejects it.
    let external = !claim.hold.is_empty()
        || !claim.call.is_empty()
        || !claim.writes.is_empty()
        || claim
            .capture_sources
            .as_ref()
            .is_some_and(|rows| !rows.is_empty());
    if !external {
        closures::self_contained(module, id, budget)?;
    }
    budget.spend(f.locals.len().saturating_add(f.values.len()))?;
    for ty in f.locals.iter().map(|local| &local.ty).chain(&f.values) {
        if let Type::Array { element, length } = ty
            && element.as_ref() == &Type::Closure(id)
            && *length != 1
        {
            return Err(unsupported(
                "forwarded F only uses the existing internal single-element owning slot",
                Some(f.span),
            ));
        }
    }
    budget.spend(f.parameters.len())?;
    for (position, parameter_local) in f.parameters.iter().enumerate() {
        if position == index {
            continue;
        }
        let ty = &f
            .locals
            .get(parameter_local.0 as usize)
            .ok_or_else(|| bad(f, "forwarding companion is outside the local table"))?
            .ty;
        if owned_callbacks::root(ty) {
            owned_callbacks::value(module, ty, Some(f.span), budget)?;
        } else {
            let work = closures::type_layout_work(ty, budget)?;
            budget.spend(work)?;
            check_type(ty, Some(f.span))?;
        }
    }
    if external {
        let leaves = sources::capture_leaves(module, id, &budget.limits.clone(), budget)?;
        budget.returned_environment_sources = true;
        // A forwarded capture is selected from the incoming environment, even
        // when its result position is a direct capture. The stored-target
        // solver must substitute that actual capture path; ordinary address
        // roots describe the environment itself, not its stored targets.
        budget.nested_callable_sources = true;
        owned_callbacks::signature(module, definition, budget)?;
        return sources::parameter_envelope(module, f, id, &leaves, budget);
    }
    budget.spend(definition.captures.len())?;
    let mut cleanup = Vec::new();
    for (index, ty) in definition.captures.iter().enumerate() {
        match ty {
            Type::Managed(_) => cleanup.push(index as u32),
            Type::OwnedDyn { interface } => {
                dynamic_dispatch::closed_owned(module, *interface, budget)?;
                cleanup.push(index as u32);
            }
            Type::Package { .. } => {
                packages::check_capture_type(module, ty, Some(f.span), budget)?;
                cleanup.push(index as u32);
            }
            Type::Closure(child) => {
                closures::self_contained(module, *child, budget)?;
                cleanup.push(index as u32);
            }
            ty if budget.inline_store_root(ty)?
                && dynamic_store::inline_sum_capture(ty, budget)? =>
            {
                dynamic_store::check_owned_capture(module, ty, None, budget)?;
                cleanup.push(index as u32);
            }
            ty if ty.is_copy_value()
                || stored_callables::inline_qualified_capture(module, ty, budget)? => {}
            _ => {
                return Err(unsupported(
                    "forwarded environment has an external capture dependency",
                    Some(f.span),
                ));
            }
        }
    }
    if claim.cleanup != cleanup {
        return Err(bad(
            f,
            "forwarded cleanup does not match actual owned capture fields",
        ));
    }
    Ok(())
}

/// General SSA typing and responsibility checking still establish exact value
/// types and one transfer at Return. This only checks the finite factory scope;
/// it does not replace those independent checks with a producer summary.
pub(super) fn check_body(
    module: &TypedModule,
    f: &TypedFunction,
    graph: &FunctionGraph,
    budget: &mut Budget,
) -> Check<()> {
    let Some(contract) = &f.callable_return_contract else {
        return Ok(());
    };
    check_dependencies(module, f, budget)?;
    if let CallableReturnSource::Parameter(parameter) = contract.source {
        return provenance::check(module, f, graph, parameter, budget);
    }
    let mut creations = 0usize;
    let mut returns = 0usize;
    for (b, block) in f.blocks.iter().enumerate() {
        budget.spend(1)?;
        if !graph.reachable[b] {
            continue;
        }
        for instruction in &block.instructions {
            budget.spend(1)?;
            match instruction.kind {
                InstructionKind::MakeClosure { closure, .. } if closure == contract.closure => {
                    creations += 1;
                }
                InstructionKind::Call { function, .. } => {
                    let target = module.functions.get(function.0 as usize).ok_or_else(|| {
                        bad(f, "factory call target is outside the function table")
                    })?;
                    if target
                        .callable_return_contract
                        .as_ref()
                        .is_some_and(|c| c.closure == contract.closure)
                    {
                        creations += 1;
                    }
                }
                _ => {}
            }
        }
        if matches!(block.terminator, Terminator::Return(Some(_))) {
            returns += 1;
        }
    }
    if creations == 0 || returns == 0 {
        return Err(bad(
            f,
            "factory requires a concrete creation or grounded forwarding site and a reachable same-type return",
        ));
    }
    Ok(())
}

/// A raw producer cannot claim a finite factory identity by making factories
/// endorse each other. This independent graph walk does not consume a Types or
/// Analyze summary and does not recurse on the host stack.
fn check_dependencies(
    module: &TypedModule,
    root: &TypedFunction,
    budget: &mut Budget,
) -> Check<()> {
    let mut active = std::collections::BTreeSet::new();
    let mut complete = std::collections::BTreeSet::new();
    let mut pending = vec![(root.id, false)];
    while let Some((id, exiting)) = pending.pop() {
        budget.spend(1)?;
        if exiting {
            active.remove(&id);
            complete.insert(id);
            continue;
        }
        if complete.contains(&id) {
            continue;
        }
        if !active.insert(id) {
            return Err(bad(
                root,
                "recursive callable factory dependencies cannot establish a finite identity",
            ));
        }
        pending.push((id, true));
        let function = module
            .functions
            .get(id.0 as usize)
            .ok_or_else(|| bad(root, "factory dependency is outside the function table"))?;
        let mut dependencies = std::collections::BTreeSet::new();
        for block in &function.blocks {
            budget.spend(1)?;
            for instruction in &block.instructions {
                budget.spend(1)?;
                if let InstructionKind::Call {
                    function: callee, ..
                } = instruction.kind
                {
                    let target = module.functions.get(callee.0 as usize).ok_or_else(|| {
                        bad(root, "factory call target is outside the function table")
                    })?;
                    if target.callable_return_contract.is_some() {
                        dependencies.insert(callee);
                    }
                }
            }
        }
        for dependency in dependencies {
            pending.push((dependency, false));
        }
    }
    Ok(())
}

pub(super) fn check_sources(
    module: &TypedModule,
    graphs: &[FunctionGraph],
    limits: &Limits,
    budget: &mut Budget,
) -> Check<()> {
    sources::check_bodies(module, graphs, limits, budget)?;
    Ok(())
}
