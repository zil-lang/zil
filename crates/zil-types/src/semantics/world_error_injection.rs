//! Implicit ErrorUnion formation in the receiving world of an actual call.
//! Core expression and original expression identifiers are separate domains.
use super::*;

pub(super) fn call_operand(
    world: &mut World<'_, '_>,
    instance: &SemanticInstance,
    function: &TypedFunction,
    sites: &[&SemanticSite],
    result: ValueId,
    is_error: bool,
    ty: &Type,
) -> R<Option<SharedType>> {
    let ledger = world.ledger;
    let local = ledger.original.borrow();
    let (program, proof) = if let Some(proof) = local
        .as_ref()
        .filter(|proof| proof.complete.contains(&instance.definition))
    {
        (world.program, proof)
    } else if let Some(frozen) = &ledger.frozen {
        (&frozen.semantic, &frozen.original)
    } else {
        return Ok(None);
    };

    let mut selected = None;
    world.pay(sites.len())?;
    for site in sites {
        if site.instance != instance.id
            || !matches!(
                site.action,
                SemanticSiteAction::Call { .. } | SemanticSiteAction::BodyCall { .. }
            )
        {
            continue;
        }
        let key = (instance.definition, site.recipe);
        world.pay(index::search(proof.call_injections.len()))?;
        let Some((arity, injected)) = proof.call_injections.get(&key) else {
            continue;
        };
        let SemanticSiteAddress::Typed {
            block,
            instruction,
            edge: SemanticSiteEdge::After,
        } = site.at
        else {
            continue;
        };
        let actual = function
            .blocks
            .get(block.0 as usize)
            .filter(|actual| actual.id == block)
            .and_then(|block| block.instructions.get(instruction as usize))
            .ok_or_else(|| bad("implicit injection call occurrence has no actual instruction"))?;
        let arguments = match &actual.kind {
            InstructionKind::Call { arguments, .. }
            | InstructionKind::SuspendCall { arguments, .. }
            | InstructionKind::ForeignCall { arguments, .. } => arguments,
            _ => continue,
        };
        // A method's implicit receiver has its own complete argument mapping.
        // This receipt describes positional operands of ordinary calls.
        if arguments.len() != *arity {
            continue;
        }
        world.pay(
            injected
                .len()
                .saturating_add(index::search(proof.method_calls.len())),
        )?;
        if proof.method_calls.contains_key(&key) {
            continue;
        }
        for &(ordinal, node) in injected {
            let Some(&argument) = arguments.get(ordinal) else {
                return Err(bad(
                    "original injection has no positional actual call operand",
                ));
            };
            if argument != result
                && !borrow_roles::prepared_argument_matches(world, function, argument, result)?
            {
                continue;
            }
            world.pay(index::search(proof.injections.len()))?;
            let injection = proof
                .injections
                .get(&(instance.definition, node))
                .ok_or_else(|| bad("original call operand loses its checked injection"))?;
            if injection.is_error != is_error {
                return Err(bad(
                    "actual injection changes its original call operand arm",
                ));
            }
            let source = SemanticOriginalTypeSource::Expression(node);
            proof.constructor_type_recipe(
                program,
                instance.definition,
                instance.definition,
                &source,
                world.limits,
                world.left,
            )?;
            let recipe = proof.type_recipe_view(
                program,
                instance.definition,
                instance.definition,
                &source,
                world.limits,
                world.left,
            )?;
            let value = world.physical_type(instance.id, &recipe, ty)?;
            if let Some(previous) = &selected {
                world.equal(previous, &value)?;
            }
            selected = Some(value);
        }
    }
    Ok(selected)
}
