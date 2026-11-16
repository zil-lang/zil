//! Whole opaque Task free-domain inventory before semantic capture admission.
//! This proves no closing state, synchronous cleanup, parent loan or access.
use super::*;

pub(super) fn zero(
    module: &TypedModule,
    payload: &Type,
    limits: &Limits,
    b: &mut Budget,
) -> CompileResult<()> {
    let Type::Task {
        result,
        retained_life,
    } = payload
    else {
        return Err(bad(
            "semantic Task inventory selects another payload family",
        ));
    };
    crate::dyn_metadata::ty(payload, b)?;
    if retained_life.binder_count != 0
        || !retained_life.ports.is_empty()
        || !retained_life.retained.is_empty()
        || !retained_life.nominal_arguments.is_empty()
    {
        return Err(vec![Diagnostic::unsupported(
            Phase::Analyze,
            "nonzero Task scope needs its complete actual result/input/phantom directory",
            None,
        )]);
    }
    // Prepay the whole raw type inventory before the existing worker graph can
    // recurse, hash or clone any actual value outside a source type-use row.
    for function in &module.functions {
        crate::dyn_metadata::ty(&function.return_type, b)?;
        for ty in function
            .values
            .iter()
            .chain(function.locals.iter().map(|local| &local.ty))
        {
            crate::dyn_metadata::ty(ty, b)?;
        }
    }
    if b.task_input_scopes.is_none() {
        b.task_input_scopes = Some(input_scope::build(module, limits, b)?);
    }
    if input_scope::domain(result, b)? != input_scope::Domain::Known(0) {
        return Err(vec![Diagnostic::unsupported(
            Phase::Analyze,
            "Task result scope is nonzero or lacks its complete actual inventory",
            None,
        )]);
    }
    let mut definitions = 0usize;
    for resource in &module.resources {
        crate::dyn_metadata::ty(&resource.payload, b)?;
        if resource.payload != *payload {
            continue;
        }
        b.spend(
            resource.name.len().saturating_add(96),
            "semantic canonical Task definition",
        )?;
        let expected = format!("{TASK_RESOURCE_NAME_PREFIX}{}", payload.fingerprint());
        if resource.name != expected || resource.teardown.is_some() {
            return Err(bad(
                "semantic Task payload has another allocator identity or user cleanup",
            ));
        }
        definitions += 1;
    }
    if definitions != 1 {
        return Err(bad(
            "semantic Task scope has no exact unique sealed resource definition",
        ));
    }
    let mut constructors = 0usize;
    for function in &module.functions {
        for block in &function.blocks {
            for instruction in &block.instructions {
                b.spend(1, "semantic actual Task creator inventory")?;
                let (id, arguments, ports) = match &instruction.kind {
                    InstructionKind::TaskCreate {
                        worker,
                        arguments,
                        port_bindings,
                    }
                    | InstructionKind::TaskCreateOwned {
                        worker,
                        arguments,
                        port_bindings,
                    } => (*worker, arguments, port_bindings),
                    _ => continue,
                };
                let actual = instruction
                    .result
                    .and_then(|value| function.values.get(value.0 as usize));
                if !matches!(actual, Some(Type::ErrorUnion { result, .. }) if matches!(result.as_ref(),Type::Managed(inner) if inner.as_ref() == payload))
                {
                    continue;
                }
                let target = module
                    .functions
                    .get(id.0 as usize)
                    .filter(|target| target.id == id)
                    .ok_or_else(|| bad("semantic Task creator has no exact actual worker"))?;
                if target.return_type != **result
                    || target.parameters.len() != arguments.len()
                    || !ports.is_empty()
                {
                    return Err(bad(
                        "zero Task frame differs from its actual worker result, inputs or retained ports",
                    ));
                }
                for (argument, parameter) in arguments.iter().zip(&target.parameters) {
                    let actual = function
                        .values
                        .get(argument.0 as usize)
                        .ok_or_else(|| bad("Task creator input SSA is absent"))?;
                    let expected = &target
                        .locals
                        .get(parameter.0 as usize)
                        .ok_or_else(|| bad("Task worker input local is absent"))?
                        .ty;
                    if actual != expected
                        || input_scope::domain(actual, b)? != input_scope::Domain::Known(0)
                    {
                        return Err(bad(
                            "zero Task frame has a nonzero or unknown complete actual input domain",
                        ));
                    }
                }
                worker(module, id, b, instruction.span)?;
                constructors += 1;
            }
        }
    }
    if constructors == 0 {
        return Err(vec![Diagnostic::unsupported(
            Phase::Analyze,
            "Task scope needs its real worker/creator inventory across the owning input boundary",
            None,
        )]);
    }
    Ok(())
}

/// Qualify a nonzero Task as a complete opaque type. The actual task kernel
/// separately proves phase, original input sources, liveness and final cleanup.
pub(super) fn complete(module: &TypedModule, payload: &Type, limits: &Limits, b: &mut Budget) -> CompileResult<()> {
    let Type::Task { result, retained_life } = payload else {
        return Err(bad("semantic Task scope changes its sealed payload family"));
    };
    if retained_life.binder_count == 0 { return zero(module, payload, limits, b); }
    crate::dyn_metadata::ty(payload, b)?;
    let mut definitions = 0usize;
    for resource in &module.resources {
        crate::dyn_metadata::ty(&resource.payload, b)?;
        if resource.payload != *payload { continue; }
        b.spend(resource.name.len().saturating_add(96), "semantic complete Task resource")?;
        if resource.name != format!("{TASK_RESOURCE_NAME_PREFIX}{}", payload.fingerprint()) || resource.teardown.is_some() {
            return Err(bad("semantic Task has another allocator identity or user cleanup"));
        }
        definitions += 1;
    }
    if definitions != 1 { return Err(bad("complete Task scope has no unique exact sealed resource definition")); }
    let mut constructors = 0usize;
    for function in &module.functions {
        for block in &function.blocks {
            for instruction in &block.instructions {
                b.spend(1, "semantic complete Task creator")?;
                let (id, arguments, bindings) = match &instruction.kind {
                    InstructionKind::TaskCreate { worker, arguments, port_bindings }
                    | InstructionKind::TaskCreateOwned { worker, arguments, port_bindings } => (*worker, arguments, port_bindings),
                    _ => continue,
                };
                let output = instruction.result.and_then(|id| function.values.get(id.0 as usize));
                if !matches!(output, Some(Type::ErrorUnion { result, .. }) if matches!(result.as_ref(), Type::Managed(inner) if inner.as_ref() == payload)) { continue; }
                let target = module.functions.get(id.0 as usize).filter(|f| f.id == id)
                    .ok_or_else(|| bad("complete Task creator has no actual worker"))?;
                if target.return_type != **result || target.parameters.len() != arguments.len() {
                    return Err(bad("complete Task changes its actual worker result or input arity"));
                }
                // This whole expected shape is reconstructed from the worker,
                // including P_i/Result rows. Ports alone are never the domain.
                if owned::borrowed_shape(target, b)? != *retained_life {
                    return Err(bad("Task retained domain changes its actual complete worker input/result directory"));
                }
                let mut expected_bindings = Vec::new();
                for (ordinal, (argument, local)) in arguments.iter().zip(&target.parameters).enumerate() {
                    let actual = function.values.get(argument.0 as usize).ok_or_else(|| bad("Task actual input is absent"))?;
                    let expected = &target.locals.get(local.0 as usize).ok_or_else(|| bad("Task worker input is absent"))?.ty;
                    crate::dyn_metadata::ty(actual, b)?;
                    crate::dyn_metadata::ty(expected, b)?;
                    if actual != expected { return Err(bad("Task actual input differs from its complete worker type")); }
                    if matches!(expected, Type::Address { inner, .. } if owned::retained_pointee(inner)) {
                        b.spend(2, "Task complete retained source selector")?;
                        expected_bindings.push(zil_ir::RetainedPortBinding {
                            port: u32::try_from(expected_bindings.len()).map_err(|_| bad("Task port ordinal overflows"))?,
                            source: zil_ir::StoragePosition { base: zil_ir::StorageBase::Parameter(
                                u32::try_from(ordinal).map_err(|_| bad("Task input ordinal overflows"))?), path: Vec::new() },
                        });
                    } else if !input_scope::known_zero(module, expected, b)? {
                        return Err(vec![Diagnostic::unsupported(Phase::Analyze,
                            "Task input needs its complete nonzero child/phantom source mapping", None)]);
                    }
                }
                if *bindings != expected_bindings { return Err(bad("Task ports change their exact ordered original input selectors")); }
                worker(module, id, b, instruction.span)?;
                constructors += 1;
            }
        }
    }
    if constructors == 0 { return Err(vec![Diagnostic::unsupported(Phase::Analyze,
        "complete Task scope needs its actual worker/creator inventory", None)]); }
    Ok(())
}
