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
    let mut occurrence = None;
    for site in world.point_sites(sites, block, instruction)? {
        if matches!(
            site.action,
            SemanticSiteAction::Call { .. } | SemanticSiteAction::BodyCall { .. }
        ) {
            if occurrence.replace(site.recipe).is_some() {
                return Err(bad("actual call repeats its original source occurrence"));
            }
        }
    }
    let occurrence =
        occurrence.ok_or_else(|| bad("actual call has no original source occurrence"))?;
    let ledger = world.ledger;
    let requests = ledger.instance_requests.borrow();
    world.pay(world.program.instances.len().saturating_add(requests.len()))?;
    let mut requested = None;
    let mut ordinary = None;
    let mut ordinary_ambiguous = false;
    for instance in &world.program.instances {
        if !matches!(instance.target, SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(id)) if id == target)
        {
            continue;
        }
        world.pay(1)?;
        if let Some(request) = requests.get(&instance.id) {
            if *request == (caller.id, occurrence, target) {
                if requested.replace(instance).is_some() {
                    return Err(bad(
                        "actual call has multiple worlds for its exact source request",
                    ));
                }
            }
        } else {
            ordinary_ambiguous |= ordinary.replace(instance).is_some();
        }
    }
    if let Some(instance) = requested {
        return Ok(instance);
    }
    if !ordinary_ambiguous {
        if let Some(instance) = ordinary {
            return Ok(instance);
        }
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
    world.pay(requests.len().saturating_add(2))?;
    let mut located = false;
    for instance in [first, other] {
        let Some((caller, site, actual)) = requests.get(&instance.id) else {
            continue;
        };
        if *actual != target {
            return Err(bad(
                "source world request changes its actual function target",
            ));
        }
        world.pay(world.program.sites.len())?;
        let mut occurrences = world.program.sites.iter().filter(|row| {
            row.instance == *caller
                && row.recipe == *site
                && matches!(
                    row.action,
                    SemanticSiteAction::Call { .. } | SemanticSiteAction::BodyCall { .. }
                )
        });
        if occurrences.next().is_none() || occurrences.next().is_some() {
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
    world.pay(world.program.sites.len().saturating_add(body.blocks.len()))?;
    let mut mints = world.program.sites.iter().filter(|site| {
        site.instance == creator.id
            && matches!(&site.action, SemanticSiteAction::Closure {
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
    let op = body
        .blocks
        .iter()
        .find(|row| row.id == block)
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
