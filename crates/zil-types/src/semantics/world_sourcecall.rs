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
