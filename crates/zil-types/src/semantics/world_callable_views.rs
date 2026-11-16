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
    let source = source
        .ok_or_else(|| pending("borrowed callee lacks its exact original whole-place receipt"))?;
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
