//! A compiler borrow used to invoke a whole Source environment must still
//! select that exact original place. Pointee type equality alone is insufficient.
use super::*;

pub(super) fn check(
    world: &mut World<'_, '_>,
    caller: &SemanticInstance,
    original: &SemanticDefinition,
    call: &SemanticSite,
    node: zil_base::NodeId,
    raw: &SemanticTypeValue,
    values: &[Option<SharedType>],
    locals: &[Option<SharedType>],
) -> R<()> {
    let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(id)) = caller.target else {
        return Err(bad("borrowed callee has no actual caller frame"));
    };
    let function = world.function(id)?;
    let SemanticDefinitionBody::FunctionWithBody { site_nodes, .. } = &original.body else {
        return Err(bad(
            "borrowed callee has no complete original occurrence directory",
        ));
    };
    let SemanticSiteAddress::Typed {
        block, instruction, ..
    } = call.at
    else {
        return Err(bad("borrowed callee has no exact actual call boundary"));
    };
    let mut source = None;
    world.pay(site_nodes.len())?;
    for occurrence in site_nodes
        .iter()
        .filter(|row| row.node == SemanticOriginalNode::Expression(node))
    {
        world.pay(world.program.sites.len())?;
        for site in world
            .program
            .sites
            .iter()
            .filter(|site| site.instance == caller.id && site.recipe == occurrence.site)
        {
            let SemanticSiteAddress::Typed {
                block: before,
                instruction: at,
                edge: SemanticSiteEdge::Before,
            } = site.at
            else {
                continue;
            };
            if before != block || at > instruction {
                continue;
            }
            if let SemanticSiteAction::TypeUse { subject, .. } = &site.action {
                if source.as_ref().is_some_and(|old| old != subject) {
                    return Err(bad(
                        "borrowed callee has conflicting actual original place selections",
                    ));
                }
                source = Some(subject.clone());
            }
        }
    }
    if source.is_none() {
        // A prepared member address has no logical Closure TypeUse: inventing
        // one would compare Address<Closure> with Closure. Select its private
        // emitter receipt only through this exact OriginalCall's actual opcode.
        let recipe = world.original_recipe(original, call.recipe)?;
        if !matches!(recipe.kind, SemanticSiteRecipeKind::OriginalCall)
            || !matches!(call.at, SemanticSiteAddress::Typed { edge: SemanticSiteEdge::After, .. })
        {
            return Err(bad("prepared callee place is not its original call"));
        }
        let ledger = world.ledger;
        let frame = ledger.frames.get(&id)
            .ok_or_else(|| bad("prepared callee place lost its actual emission frame"))?;
        world.pay(function.blocks.len())?;
        let operation = function.blocks.iter().find(|row| row.id == block)
            .and_then(|row| row.instructions.get(instruction as usize))
            .ok_or_else(|| bad("prepared callee place has no actual call opcode"))?;
        let InstructionKind::CallClosureBorrowed { callee, .. } = &operation.kind else {
            return Err(bad("prepared callee place changed its borrowed call opcode"));
        };
        world.pay(frame.expressions.len())?;
        let mut selected = None;
        for row in frame.expressions.values() {
            let Some(address) = row.callee_place else { continue; };
            world.pay(row.instructions.len())?;
            if row.callee.is_none() || !row.instructions.iter().any(|point|
                point.block == block && point.instruction == instruction as usize)
                || !row.end.is_some_and(|end| end.block == block && end.instruction > instruction as usize)
            { continue; }
            if address != *callee || selected.replace(address).is_some() {
                return Err(bad("prepared callee place changes its exact call or address"));
            }
        }
        let address = selected.ok_or_else(|| pending(
            "borrowed callee lacks its exact original whole-place receipt"))?;
        let value = world.get(values, address)?;
        world.pay(1)?;
        // Actual projection/permission/type checks have already formed this
        // address. The receipt only preserves which place this call selected;
        // it grants no ownership or permission and cannot select by type.
        let selected = roles::outer(world, &value)?;
        let expected = roles::outer(world, raw)?;
        for source in [&selected, &expected] {
            if let roles::Source::Class(class) = source {
                crate::dyn_metadata::semantic_operand_snapshot(class, world.limits, world.left, 1)?;
            }
        }
        if selected != expected {
            return Err(bad("prepared callee place changes its complete actual address occurrence"));
        }
        return Ok(());
    }
    let source = source.expect("checked original place selection");
    let mut image = roles::outer(world, raw)?;
    let mut seen = std::collections::BTreeSet::new();
    loop {
        let roles::Source::Class(SemanticClassExpr::OuterOccurrence {
            operand: SemanticOperand::Value(head),
            path,
            ..
        }) = image
        else {
            return Err(bad("borrowed callee lacks its real address mint"));
        };
        if !path.is_empty() || seen.len() >= world.limits.max_call_depth || !seen.insert(head) {
            return Err(bad(
                "borrowed callee has a cyclic or invalid actual borrow path",
            ));
        }
        world.get(values, head)?;
        world.pay(function.blocks.len())?;
        let mut producer = None;
        for row in &function.blocks {
            world.pay(row.instructions.len())?;
            for operation in &row.instructions {
                if operation.result == Some(head) && producer.replace(&operation.kind).is_some() {
                    return Err(bad("borrowed callee address has multiple actual producers"));
                }
            }
        }
        let producer =
            producer.ok_or_else(|| bad("borrowed callee address has no actual producer"))?;
        match producer {
            InstructionKind::ReborrowAddress { address, .. } => {
                let parent = world.get(values, *address)?;
                image = roles::outer(world, &parent)?;
                continue;
            }
            InstructionKind::BorrowCapture { index, .. }
                if source == SemanticOperand::Capture(*index) =>
            {
                world.capture(caller, *index)?;
            }
            InstructionKind::BorrowLocal { local } | InstructionKind::BorrowLocalMut { local } => {
                world.local(locals, *local)?;
                let matches = match &source {
                    SemanticOperand::Local(expected) => expected == local,
                    SemanticOperand::Parameter(parameter) => {
                        function.parameters.get(*parameter as usize) == Some(local)
                    }
                    SemanticOperand::Value(value) => {
                        let mut initializer = None;
                        for row in &function.blocks {
                            world.pay(row.instructions.len())?;
                            for operation in &row.instructions {
                                if let InstructionKind::Store {
                                    local: target,
                                    value,
                                } = operation.kind
                                    && target == *local
                                    && initializer.replace(value).is_some()
                                {
                                    return Err(bad(
                                        "temporary callee place has several actual definitions",
                                    ));
                                }
                            }
                        }
                        initializer == Some(*value)
                    }
                    _ => false,
                };
                if !matches {
                    return Err(bad("borrowed callee selects another original local"));
                }
            }
            InstructionKind::BorrowOwnedArrayElement {
                local, index: 0, ..
            } if source == SemanticOperand::Local(*local) => {
                world.local(locals, *local)?;
            }
            _ => {
                return Err(pending(
                    "borrowed callee needs its complete original storage projection",
                ));
            }
        }
        return Ok(());
    }
}
