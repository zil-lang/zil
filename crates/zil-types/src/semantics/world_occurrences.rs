//! One selector for immutable original expressions and their real emitted
//! occurrences. These receipts identify a value/place; they grant no type,
//! permission, lifetime equation or current reaching-world fact.
use super::*;
use zil_base::NodeId;

pub(super) fn node(
    world: &mut World<'_, '_>,
    original: &SemanticDefinition,
    site: &SemanticSite,
) -> R<Option<NodeId>> {
    Ok(match original_node(world, original.id, site.recipe)? {
        Some(SemanticOriginalNode::Expression(node)) => Some(node),
        _ => None,
    })
}

pub(super) fn original_node(
    world: &mut World<'_, '_>,
    definition: SemanticDefinitionId,
    site: SemanticSiteId,
) -> R<Option<SemanticOriginalNode>> {
    Ok(world.original_site(definition, site)?.node)
}

/// Select the exact value of a relocated site, including calls whose original
/// expression has no separate TypeUse. The action match is deliberately total.
pub(super) fn site_value(
    world: &mut World<'_, '_>,
    instance: &SemanticInstance,
    original: &SemanticDefinition,
    site: &SemanticSite,
) -> R<Option<ValueId>> {
    if site.instance != instance.id {
        return Err(bad("occurrence changes its actual world"));
    }
    let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(id)) = instance.target
    else {
        return Err(bad("value occurrence has no actual function"));
    };
    let function = world.function(id)?;
    let SemanticSiteAddress::Typed {
        block,
        instruction,
        edge,
    } = site.at
    else {
        return Err(bad("value occurrence retains a Core boundary"));
    };
    let (value, operation) = match &site.action {
        SemanticSiteAction::TypeUse { subject, .. } => {
            if edge != SemanticSiteEdge::Before {
                return Err(bad("value TypeUse changes its actual Before boundary"));
            }
            let SemanticOperand::Value(value) = subject else {
                return Ok(None);
            };
            (*value, false)
        }
        SemanticSiteAction::BodyCall { .. }
        | SemanticSiteAction::Call { .. }
        | SemanticSiteAction::CallableCall { .. } => {
            if !matches!(
                world.original_recipe(original, site.recipe)?.kind,
                SemanticSiteRecipeKind::OriginalCall
                    | SemanticSiteRecipeKind::Call { .. }
                    | SemanticSiteRecipeKind::CallableCall { .. }
            ) {
                return Err(bad("call result occurrence changes its immutable recipe"));
            }
            if edge != SemanticSiteEdge::After {
                return Err(bad(
                    "call result occurrence changes its actual After boundary",
                ));
            }
            let value = world.block(function, block)?
                .and_then(|row| row.instructions.get(instruction as usize))
                .and_then(|row| row.result)
                .ok_or_else(|| bad("call occurrence has no exact actual result SSA"))?;
            (value, true)
        }
        // Construction and environment mints have their own operation
        // receipts. A loan query consumes their later observed whole value;
        // it must not substitute a pre-adaptation construction SSA for it.
        SemanticSiteAction::Construct { .. } | SemanticSiteAction::Closure { .. } => {
            return Ok(None);
        }
        SemanticSiteAction::DeclareLocal { .. } => return Ok(None),
        SemanticSiteAction::CoreBodyCall => {
            return Err(bad("unformed original Call survives occurrence selection"));
        }
    };
    world.pay(index::search(world.index.expressions.len()))?;
    let emitted = world
        .index
        .expressions
        .get(&(id, value))
        .ok_or_else(|| bad("original value occurrence has no actual emitter receipt"))?;
    index::pay(world.left, emitted.len())?;
    let emitted = emitted.clone();
    let ledger = world.ledger;
    let frame = ledger
        .frames
        .get(&id)
        .ok_or_else(|| bad("value occurrence has no exact emission frame"))?;
    let mut found = false;
    for mint in emitted {
        world.pay(index::search(frame.expressions.len()).saturating_add(1))?;
        let expression = frame
            .expressions
            .get(&mint)
            .ok_or_else(|| bad("value occurrence changes its emitter index"))?;
        world.pay(expression.instructions.len())?;
        let boundary = if operation {
            expression
                .instructions
                .iter()
                .any(|point| point.block == block && point.instruction == instruction as usize)
                && expression
                    .end
                    .is_some_and(|end| end.block == block && end.instruction > instruction as usize)
        } else {
            expression
                .end
                .is_some_and(|end| end.block == block && end.instruction == instruction as usize)
        };
        found |= expression.value == Some(value) && boundary;
    }
    if !found {
        return Err(bad(
            "original value occurrence changes its actual SSA or end boundary",
        ));
    }
    Ok(Some(value))
}

pub(super) fn expression_value(
    world: &mut World<'_, '_>,
    instance: &SemanticInstance,
    original: &SemanticDefinition,
    source: NodeId,
) -> R<Option<ValueId>> {
    expression_value_with_construction(world, instance, original, source, false)
}

/// The owner is the construction itself. Explaining a subsequent descriptor
/// must never turn its original owner's manufacturing SSA into an alias view.
pub(super) fn owning_expression_value(
    world: &mut World<'_, '_>,
    instance: &SemanticInstance,
    original: &SemanticDefinition,
    source: NodeId,
) -> R<Option<ValueId>> {
    expression_value_with_construction(world, instance, original, source, true)
}

fn expression_value_with_construction(
    world: &mut World<'_, '_>,
    instance: &SemanticInstance,
    original: &SemanticDefinition,
    source: NodeId,
    include_construction: bool,
) -> R<Option<ValueId>> {
    world.pay(index::search(world.index.sites.len()))?;
    let Some(offsets) = world.index.sites.get(&instance.id) else {
        return Ok(None);
    };
    index::pay(world.left, offsets.len())?;
    let offsets = offsets.clone();
    let program = world.program;
    let mut selected = None;
    for offset in offsets {
        let site = &program.sites[offset];
        if node(world, original, site)? != Some(source) {
            continue;
        }
        let value = if include_construction
            && let SemanticSiteAction::Construct { result: SemanticOperand::Value(value), .. } = &site.action
        {
            // The normal constructor world has checked the actual After
            // operation/nominal fields in this same reaching state.
            Some(*value)
        } else {
            site_value(world, instance, original, site)?
        };
        let Some(value) = value else { continue; };
        if selected.replace(value).is_some_and(|old| old != value) {
            return Err(bad(
                "one original expression selects different actual SSA mints",
            ));
        }
    }
    Ok(selected)
}

/// Callee places are selected before arguments. A compound member borrow has
/// no logical Closure TypeUse; the existing call receipt retains its real full
/// projection and address instead of asking a root-only reader to recreate it.
pub(super) fn prepared_callee(
    world: &mut World<'_, '_>,
    instance: &SemanticInstance,
    original: &SemanticDefinition,
    call: &SemanticSite,
    source: NodeId,
) -> R<Option<ValueId>> {
    if !matches!(
        world.original_recipe(original, call.recipe)?.kind,
        SemanticSiteRecipeKind::OriginalCall
    ) {
        return Err(bad("prepared callee is not its immutable original Call"));
    }
    let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(id)) = instance.target
    else {
        return Err(bad("prepared callee has no actual caller"));
    };
    let SemanticSiteAddress::Typed {
        block,
        instruction,
        edge: SemanticSiteEdge::After,
    } = call.at
    else {
        return Err(bad("prepared callee has no actual call boundary"));
    };
    let ledger = world.ledger;
    let proof = ledger.original.borrow();
    let proof = proof
        .as_ref()
        .ok_or_else(|| bad("prepared callee has no original Gamma"))?;
    world.pay(index::search(proof.callable_calls.len()))?;
    if !proof
        .callable_calls
        .get(&(original.id, call.recipe))
        .is_some_and(|row| row.callee_node == source)
    {
        return Err(bad(
            "prepared callee selects another original callable operand",
        ));
    }
    let function = world.function(id)?;
    let operation = world.block(function, block)?
        .and_then(|row| row.instructions.get(instruction as usize))
        .ok_or_else(|| bad("prepared callee has no actual opcode"))?;
    let InstructionKind::CallClosureBorrowed { callee, .. } = operation.kind else {
        return Err(bad(
            "prepared callee changes its actual borrowed call opcode",
        ));
    };
    let ledger = world.ledger;
    let frame = ledger
        .frames
        .get(&id)
        .ok_or_else(|| bad("prepared callee has no emission frame"))?;
    let key = ledger.original_call_expression(id, original, call.recipe, world.left)?;
    world.pay(index::search(frame.expressions.len()))?;
    let row = frame.expressions.get(&key)
        .ok_or_else(|| bad("prepared callee lacks its exact actual Core Call receipt"))?;
    world.pay(row.instructions.len())?;
    if !row.instructions.iter().any(|point| point.block == block && point.instruction == instruction as usize)
        || !row.end.is_some_and(|end| end.block == block && end.instruction > instruction as usize) {
        return Err(bad("prepared callee changes its exact actual Call instruction or end boundary"));
    }
    let address = row.callee_place.ok_or_else(|| bad("borrowed whole callee lacks its exact prepared place receipt"))?;
    if row.callee.is_none() || address != callee {
        return Err(bad("prepared callee changes its exact selected address or call"));
    }
    Ok(Some(address))
}
