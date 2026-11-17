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
    let SemanticDefinitionBody::FunctionWithBody { site_nodes, .. } = &original.body else {
        return Ok(None);
    };
    world.pay(site_nodes.len())?;
    let mut rows = site_nodes.iter().filter(|row| row.site == site.recipe);
    let Some(row) = rows.next() else {
        return Ok(None);
    };
    if rows.next().is_some() {
        return Err(bad(
            "original occurrence has multiple immutable source nodes",
        ));
    }
    Ok(match row.node {
        SemanticOriginalNode::Expression(node) => Some(node),
        SemanticOriginalNode::Statement(_) => None,
    })
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
            world.pay(function.blocks.len())?;
            let value = function
                .blocks
                .iter()
                .find(|row| row.id == block)
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
        let Some(value) = site_value(world, instance, original, site)? else {
            continue;
        };
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
    world.pay(function.blocks.len())?;
    let operation = function
        .blocks
        .iter()
        .find(|row| row.id == block)
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
    world.pay(frame.expressions.len())?;
    let mut selected = None;
    for row in frame.expressions.values() {
        let Some(address) = row.callee_place else {
            continue;
        };
        world.pay(row.instructions.len())?;
        if !row
            .instructions
            .iter()
            .any(|point| point.block == block && point.instruction == instruction as usize)
            || !row
                .end
                .is_some_and(|end| end.block == block && end.instruction > instruction as usize)
        {
            continue;
        }
        if row.callee.is_none() || address != callee || selected.replace(address).is_some() {
            return Err(bad(
                "prepared callee changes its exact selected address or call",
            ));
        }
    }
    Ok(selected)
}
