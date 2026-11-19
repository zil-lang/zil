//! Source requests locate an actual call's world. They do not establish its
//! mixed substitution, signature, classes, or original body theorem.
use super::*;

pub(super) fn callee<'a>(
    world: &mut World<'a, '_>,
    caller: &SemanticInstance,
    target: FunctionId,
    sites: &[&SemanticSite],
    block: BlockId,
    instruction: usize,
) -> R<&'a SemanticInstance> {
    resolve(world, caller, target, sites, block, instruction).map(|(callee, _)| callee)
}

pub(super) fn resolve<'a>(
    world: &mut World<'a, '_>,
    caller: &SemanticInstance,
    target: FunctionId,
    sites: &[&SemanticSite],
    block: BlockId,
    instruction: usize,
) -> R<(&'a SemanticInstance, &'a SemanticSite)> {
    let mut occurrence = None;
    for site in world.point_sites(sites, block, instruction)? {
        if matches!(
            site.action,
            SemanticSiteAction::Call { .. } | SemanticSiteAction::BodyCall { .. }
        ) {
            if occurrence.replace(site).is_some() {
                return Err(bad("actual call repeats its original source occurrence"));
            }
        }
    }
    let site =
        occurrence.ok_or_else(|| bad("actual call has no original source occurrence"))?;
    let occurrence = site.recipe;
    world.pay(index::search(world.index.requested_calls.len()).saturating_add(1))?;
    if let Some(positions) = world.index.requested_calls.get(&(caller.id, occurrence, target)) {
        let [position] = positions.as_slice() else {
            return Err(bad("actual call has multiple worlds for its exact source request"));
        };
        return Ok((&world.program.instances[*position], site));
    }
    world.pay(index::search(world.index.ordinary_calls.len()).saturating_add(1))?;
    if let Some([position]) = world.index.ordinary_calls.get(&target).map(Vec::as_slice) {
        return Ok((&world.program.instances[*position], site));
    }
    let mut diagnostic = bad("actual call has no unique world for its exact original request");
    diagnostic.notes.push(format!(
        "caller {:?} definition {:?} original site {:?}; actual target {:?}, block {:?}, instruction {}",
        caller.id, caller.definition, occurrence, target, block, instruction
    ));
    Err(diagnostic)
}

pub(super) fn shared(
    world: &mut World<'_, '_>,
    first: &SemanticInstance,
    other: &SemanticInstance,
    target: FunctionId,
) -> R<()> {
    if first.definition != other.definition {
        return Err(bad(
            "one actual function changes its original definition between worlds",
        ));
    }
    let first_environment = first.environment.as_ref();
    let other_environment = other.environment.as_ref();
    if first_environment.is_some() || other_environment.is_some() {
        let (Some(first_environment), Some(other_environment)) =
            (first_environment, other_environment)
        else {
            return Err(bad("shared closure entry changes its environment identity"));
        };
        let SemanticClosureTarget::Typed(closure) = first_environment.closure else {
            return Err(bad(
                "shared closure entry lacks its actual environment identity",
            ));
        };
        if other_environment.closure != SemanticClosureTarget::Typed(closure)
            || first.parent == other.parent
        {
            return Err(bad(
                "shared closure entry changes its environment or repeats its creator world",
            ));
        }
        for child in [first, other] {
            shared_creator(world, closure, child)?;
        }
        return Ok(());
    }
    let ledger = world.ledger;
    let requests = ledger.instance_requests.borrow();
    let mut located = false;
    for instance in [first, other] {
        world.pay(index::search(requests.len()).saturating_add(1))?;
        let Some((caller, site, actual)) = requests.get(&instance.id) else {
            continue;
        };
        if *actual != target {
            return Err(bad(
                "source world request changes its actual function target",
            ));
        }
        world.pay(index::search(world.index.source_call_occurrences.len()).saturating_add(1))?;
        if world.index.source_call_occurrences.get(&(*caller, *site)) != Some(&1) {
            return Err(bad(
                "shared actual world has no unique original call occurrence",
            ));
        }
        located = true;
    }
    if !located {
        return Err(bad(
            "shared actual function has no authentic source request",
        ));
    }
    Ok(())
}

fn shared_creator(
    world: &mut World<'_, '_>,
    closure: ClosureId,
    child: &SemanticInstance,
) -> R<()> {
    world.child_at(closure, child.id)?;
    let creator = world.instance(
        child
            .parent
            .ok_or_else(|| bad("shared closure entry omits its original creator world"))?,
    )?;
    let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function)) = creator.target
    else {
        return Err(bad("shared closure creator lacks its actual function"));
    };
    let body = world.function(function)?;
    let mut mints = world.instance_sites(creator.id)?.into_iter().filter(|site| {
        matches!(&site.action, SemanticSiteAction::Closure {
                closure: SemanticClosureTarget::Typed(id), ..
            } if *id == closure)
    });
    let mint = mints
        .next()
        .ok_or_else(|| bad("shared closure entry lacks its original creator mint"))?;
    let SemanticSiteAddress::Typed {
        block,
        instruction,
        edge: SemanticSiteEdge::After,
    } = mint.at
    else {
        return Err(bad(
            "shared closure mint lacks its actual constructor boundary",
        ));
    };
    if mints.next().is_some() {
        return Err(bad("shared closure repeats its original creator mint"));
    }
    let original = world.definition(creator.definition)?;
    let recipe = world.original_recipe(original, mint.recipe)?;
    let op = world.block(body, block)?
        .and_then(|row| row.instructions.get(instruction as usize))
        .ok_or_else(|| bad("shared closure mint leaves its actual creator function"))?;
    if !matches!(&recipe.kind, SemanticSiteRecipeKind::Closure { definition, .. }
        if *definition == child.definition)
        || !matches!(op.kind, InstructionKind::MakeClosure { closure: id, .. } if id == closure)
        || !matches!(&mint.action, SemanticSiteAction::Closure { result: SemanticOperand::Value(value), .. }
            if op.result == Some(*value))
    {
        return Err(bad(
            "shared closure mint does not select its original closure and actual constructor",
        ));
    }
    Ok(())
}
