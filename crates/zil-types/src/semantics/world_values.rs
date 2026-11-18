//! Function addresses are authenticated at their original expression occurrence.
//! A matching pointer signature alone never selects a target definition.
use super::*;
use crate::semantics::original::FunctionValue;

#[allow(clippy::too_many_arguments)]
pub(super) fn reference(
    world: &mut World<'_, '_>,
    instance: &SemanticInstance,
    original: &SemanticDefinition,
    sites: &[&SemanticSite],
    block: BlockId,
    index: usize,
    result: ValueId,
    target: FunctionId,
    foreign: bool,
    ty: &Type,
    implicit: &BTreeMap<u32, SemanticClassExpr>,
) -> R<SharedType> {
    world.pay(sites.len())?;
    let mut selected: Option<FunctionValue> = None;
    for site in sites {
        if !matches!(&site.action,SemanticSiteAction::TypeUse{subject:SemanticOperand::Value(value),..} if *value==result)
        {
            continue;
        }
        let recipe = world.original_recipe(original, site.recipe)?;
        let node = match &recipe.kind {
            SemanticSiteRecipeKind::TypeUse { expected: SemanticTypeRecipe::OriginalType {
                scope: SemanticScopeRef::Ancestor(0), source: SemanticOriginalTypeSource::Expression(node),
            } } => *node,
            SemanticSiteRecipeKind::Closure { environment, .. } if environment.is_empty() => {
                occurrences::node(world, original, site)?
                    .ok_or_else(|| bad("thin function value omits its creation expression"))?
            }
            _ => continue,
        };
        let ledger = world.ledger;
        let proof = ledger.original.borrow();
        let proof = proof
            .as_ref()
            .ok_or_else(|| bad("function address has no checked original Gamma"))?;
        let Some(value) = proof.function_values.get(&(original.id, node)) else {
            continue;
        };
        if !matches!(site.at,SemanticSiteAddress::Typed{block:actual,instruction,edge:SemanticSiteEdge::Before} if actual==block && instruction as usize==index.saturating_add(1))
        {
            return Err(bad(
                "function address query is not its real emission boundary",
            ));
        }
        if selected.is_some() {
            return Err(bad(
                "function address has multiple original occurrence seals",
            ));
        }
        world.pay(value.actuals.len().saturating_add(1))?;
        for actual in &value.actuals {
            crate::dyn_metadata::semantic_operand_snapshot(actual, world.limits, world.left, 1)?;
        }
        crate::dyn_metadata::semantic_operand_snapshot(
            &value.signature,
            world.limits,
            world.left,
            1,
        )?;
        selected = Some(value.clone());
    }
    let value = selected
        .ok_or_else(|| bad("function address lacks its exact original expression TypeUse"))?;
    world.pay(world.program.instances.len())?;
    let mut targets=world.program.instances.iter().filter(|row|matches!(row.target,SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function)) if function==target));
    let callee = targets
        .next()
        .ok_or_else(|| bad("function address has no real Typed target instance"))?;
    if targets.next().is_some() || callee.definition != value.target {
        return Err(bad(
            "function address changes its unique original target definition",
        ));
    }
    let function = world.function(target)?;
    if function.environment.is_some() {
        return Err(bad("function address captures an actual environment"));
    }
    match (foreign, value.kind, &function.foreign) {
        (
            false,
            SemanticCodeKind::FunctionPointer | SemanticCodeKind::BorrowedFunctionPointer,
            None,
        ) => {}
        (true, SemanticCodeKind::ForeignCallback, Some(declaration)) if declaration.abi == "C" => {}
        _ => {
            return Err(bad(
                "function address changes its declared language or C ABI kind",
            ));
        }
    }
    let borrowed = value.signature.invocation_lifetimes != 0 && !foreign;
    if borrowed {
        borrowed_entry(world, instance, callee, function, &value, ty)?;
    } else {
        methods::sealed_worker(
            world, instance, callee, function, &value.signature, &value.actuals, implicit,
        )?;
    }
    let recipe = SemanticTypeRecipe::Code {
        kind: value.kind,
        signature: Box::new(value.signature),
    };
    let expected = world.physical_type(instance.id, &recipe, ty)?;
    if World::type_of(&expected)? != ty {
        return Err(bad(
            "function address changes its complete original pointer type",
        ));
    }
    Ok(expected)
}

fn borrowed_entry(
    world: &mut World<'_, '_>,
    caller: &SemanticInstance,
    callee: &SemanticInstance,
    function: &TypedFunction,
    value: &FunctionValue,
    pointer: &Type,
) -> R<()> {
    let Type::BorrowedFunctionPointer { parameters, result, life } = pointer else {
        return Err(bad("borrowed code address erases its invocation relation"));
    };
    let header = function.storage_life.as_ref()
        .ok_or_else(|| bad("borrowed code entry omits its complete storage header"))?;
    if function.call_kind != value.signature.call_kind
        || function.result_mode != value.signature.result_mode
        || function.parameters.len() != parameters.len()
        || callee.actuals.len() != value.actuals.len()
    {
        return Err(bad("borrowed code entry changes its original ABI or arity"));
    }
    world.pay(parameters.len().saturating_add(value.actuals.len()))?;
    for (actual, recipe) in callee.actuals.iter().zip(&value.actuals) {
        let expected = super::super::materialize::actual_in_view(
            world.program, &world.view, world.ledger, caller.id, recipe, world.limits, world.left,
        )?;
        methods::actual_specialization(world, actual, &expected)?;
    }
    for (local, expected) in function.parameters.iter().zip(parameters) {
        let actual = &function.locals.get(local.0 as usize)
            .ok_or_else(|| bad("borrowed code entry parameter has no actual local"))?.ty;
        inference::charge_type_snapshot(actual, Span::default(), world.limits, world.left, 1)?;
        inference::charge_type_snapshot(expected, Span::default(), world.limits, world.left, 1)?;
        if actual != expected {
            return Err(bad("borrowed code entry changes a complete parameter type"));
        }
    }
    inference::charge_type_snapshot(&function.return_type, Span::default(), world.limits, world.left, 1)?;
    inference::charge_type_snapshot(result, Span::default(), world.limits, world.left, 1)?;
    world.pay(crate::borrowed_fp::metadata_work(life)
        .saturating_add(crate::borrowed_fp::metadata_work(header).saturating_mul(3)))?;
    crate::borrowed_fp::qualified_inputs(header, parameters, result, |work| world.pay(work))?;
    if &function.return_type != result.as_ref()
        || crate::borrowed_fp::static_pointer_life(parameters, result, header).as_ref() != Some(life)
    {
        return Err(bad("borrowed code entry changes its complete invocation relation"));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn indirect(
    world: &mut World<'_, '_>,
    instance: &SemanticInstance,
    original: &SemanticDefinition,
    sites: &[&SemanticSite],
    block: BlockId,
    index: usize,
    callee: ValueId,
    arguments: &[ValueId],
    ty: &Type,
    values: &[Option<SharedType>],
    implicit: &BTreeMap<u32, SemanticClassExpr>,
) -> R<SharedType> {
    let callee_value = world.get(values, callee)?;
    let (parameters, result, invocations) = match World::type_of(&callee_value)? {
        Type::FunctionPointer { parameters, result } => (parameters, result, 0),
        Type::BorrowedFunctionPointer { parameters, result, life } => {
            (parameters, result, life.invocation_binder_count)
        }
        _ => return Err(pending(
            "indirect code invocation requires its complete original ABI relation",
        )),
    };
    let mut selected = None;
    for site in world.point_sites(sites, block, index)? {
        let SemanticSiteAction::BodyCall { actuals } = &site.action else {
            continue;
        };
        let scheme = {
            let ledger = world.ledger;
            let proof = ledger.original.borrow();
            let proof = proof
                .as_ref()
                .ok_or_else(|| bad("indirect call has no checked original Gamma"))?;
            let Some(scheme) = proof.callable_calls.get(&(original.id, site.recipe)) else {
                continue;
            };
            let solved = proof
                .calls
                .get(&(original.id, site.recipe))
                .ok_or_else(|| bad("indirect call loses its complete original mixed solution"))?;
            if !actuals.is_empty() || !solved.is_empty() {
                return Err(bad(
                    "indirect code call invents declaration generic actuals",
                ));
            }
            crate::dyn_metadata::semantic_operand_snapshot(
                &scheme.callee,
                world.limits,
                world.left,
                1,
            )?;
            crate::dyn_metadata::semantic_operand_snapshot(
                &scheme.signature,
                world.limits,
                world.left,
                1,
            )?;
            scheme.clone()
        };
        if selected.is_some() {
            return Err(bad("indirect code call repeats its original operation"));
        }
        if scheme.kind != CallableKind::FunctionPointer
            || scheme.signature.call_kind != CallKind::Direct
            || scheme.signature.bound_lifetimes != 0
            || scheme.signature.invocation_lifetimes != invocations
            || scheme.signature.parameters.len() != parameters.len()
            || arguments.len() != parameters.len()
            || !matches!(
                scheme.signature.result_mode,
                PassingMode::Infer | PassingMode::Copy
            )
            || result.as_ref() != ty
        {
            return Err(bad(
                "indirect code call changes its complete original ABI or arity",
            ));
        }
        let query = SemanticTypeRecipe::OriginalType {
            scope: SemanticScopeRef::Ancestor(0),
            source: SemanticOriginalTypeSource::Expression(scheme.callee_node),
        };
        let expected = world.physical_type(instance.id, &query, World::type_of(&callee_value)?)?;
        world.same(instance.id, &callee_value, &expected, implicit)?;
        world.pay(parameters.len())?;
        for ((argument, ty), parameter) in arguments
            .iter()
            .zip(parameters)
            .zip(&scheme.signature.parameters)
        {
            if !matches!(parameter.mode, PassingMode::Infer | PassingMode::Copy) {
                return Err(bad(
                    "indirect code pointer erases an original transfer mode",
                ));
            }
            let expected = world.physical_type(instance.id, &parameter.ty, ty)?;
            let actual = world.get(values, *argument)?;
            world.same(instance.id, &actual, &expected, implicit)?;
        }
        selected = Some(world.physical_type(instance.id, &scheme.signature.result, ty)?);
    }
    selected.ok_or_else(|| bad("indirect code call lacks its exact original callable scheme"))
}
