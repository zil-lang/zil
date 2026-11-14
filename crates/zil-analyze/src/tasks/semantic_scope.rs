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
