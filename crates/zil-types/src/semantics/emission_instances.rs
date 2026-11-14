//! Publish only actual emitter instances. Original definitions and their full
//! body/site inventories are never reduced by this private relocation pass.
use super::*;
use std::collections::BTreeSet;

fn request(
    instance: &SemanticInstance,
    left: &mut usize,
) -> R<Option<(SemanticInstanceId, SemanticSiteId)>> {
    let mut result = None;
    pay(left, instance.actuals.len())?;
    for actual in &instance.actuals {
        if let SemanticActual::CoreInferred { scope, call, .. } = actual {
            let key = (*scope, *call);
            if result.is_some_and(|old| old != key) {
                return Err(bad(
                    "one runtime specialization mixes original call requests",
                ));
            }
            result = Some(key);
        }
    }
    Ok(result)
}

fn source_call<'a>(
    program: &'a SemanticModule,
    caller: SemanticInstanceId,
    recipe: SemanticSiteId,
    left: &mut usize,
) -> R<&'a SemanticSite> {
    pay(left, program.sites.len())?;
    let mut found = program
        .sites
        .iter()
        .filter(|site| site.instance == caller && site.recipe == recipe);
    let site = found
        .next()
        .ok_or_else(|| bad("actual specialization has no original caller mint"))?;
    if found.next().is_some() {
        return Err(bad("actual specialization caller mint is duplicated"));
    }
    if !matches!(
        site.action,
        SemanticSiteAction::CoreBodyCall
            | SemanticSiteAction::Call { .. }
            | SemanticSiteAction::CallableCall { .. }
    ) {
        return Err(bad(
            "runtime specialization request does not select an original call",
        ));
    }
    Ok(site)
}

fn called(
    lower: &mut Lower<'_, '_>,
    caller: FunctionId,
    site: &SemanticSite,
    origin: Origin,
) -> R<FunctionId> {
    let SemanticSiteAddress::Core {
        expression,
        occurrence: 0,
        role: SemanticEmissionRole::Call,
    } = site.at
    else {
        return Err(bad(
            "specialization caller is not a real Core call occurrence",
        ));
    };
    let receipt = lower.expression(caller, expression)?;
    pay(lower.left, receipt.instructions.len())?;
    let mut found = None;
    for point in &receipt.instructions {
        let target = match lower.instruction(caller, *point)?.kind {
            InstructionKind::Call { function, .. }
            | InstructionKind::SuspendCall { function, .. }
            | InstructionKind::ForeignCall { function, .. } => function,
            _ => continue,
        };
        if lower.frame(target)?.origin == origin && found.replace(target).is_some() {
            return Err(bad(
                "one original request has multiple actual specialization calls",
            ));
        }
    }
    found.ok_or_else(|| bad("original request does not call its exact actual specialization"))
}

fn created(
    lower: &mut Lower<'_, '_>,
    program: &SemanticModule,
    instance: &SemanticInstance,
    parent: FunctionId,
) -> R<FunctionId> {
    let SemanticInstanceTarget::Function(target @ SemanticFunctionTarget::CoreClosure { .. }) =
        &instance.target
    else {
        return Err(bad("closure instance has no original closure occurrence"));
    };
    let origin = Lower::origin(target)?;
    let environment = instance
        .environment
        .as_ref()
        .ok_or_else(|| bad("closure instance lost its actual environment request"))?;
    let closure = lower.closure(&environment.closure, parent)?;
    pay(lower.left, lower.closures.len())?;
    let actual = lower
        .closures
        .iter()
        .find(|row| row.id == closure)
        .ok_or_else(|| bad("actual closure request has no definition"))?;
    let entry = actual.function;
    if lower.frame(entry)?.origin != origin || lower.frame(entry)?.parent != Some((parent, closure))
    {
        return Err(bad(
            "closure request changes its actual creator or Core occurrence",
        ));
    }
    let source_parent = instance
        .parent
        .ok_or_else(|| bad("closure request has no lexical parent"))?;
    pay(lower.left, program.sites.len())?;
    let mut mint = None;
    for site in &program.sites {
        if site.instance != source_parent {
            continue;
        }
        let SemanticSiteAction::Closure {
            closure: source, ..
        } = &site.action
        else {
            continue;
        };
        if source != &environment.closure {
            continue;
        }
        if mint.replace(site).is_some() {
            return Err(bad(
                "one child environment has multiple original creator mints",
            ));
        }
    }
    let site = mint.ok_or_else(|| bad("child environment has no exact original creator mint"))?;
    let SemanticSiteAddress::Core {
        expression,
        occurrence: 0,
        role: SemanticEmissionRole::Closure,
    } = site.at
    else {
        return Err(bad(
            "child creation does not select a real Core closure mint",
        ));
    };
    let receipt = lower.expression(parent, expression)?;
    pay(lower.left, receipt.instructions.len())?;
    let mut count = 0;
    for point in &receipt.instructions {
        if matches!(lower.instruction(parent, *point)?.kind, InstructionKind::MakeClosure { closure: actual, .. } if actual == closure)
        {
            count += 1;
        }
    }
    if count != 1 {
        return Err(bad("child environment is not its one actual MakeClosure"));
    }
    Ok(entry)
}

fn reference(value: &SemanticTypeValue) -> Option<SemanticInstanceId> {
    match value {
        SemanticTypeValue::Core { scope, .. } => Some(*scope),
        SemanticTypeValue::Resolved { .. } => None,
    }
}
fn actual_reference(value: &SemanticActual) -> Option<SemanticInstanceId> {
    match value {
        SemanticActual::Type(value) | SemanticActual::Const { ty: value, .. } => reference(value),
        SemanticActual::CoreRecipe { scope, .. } | SemanticActual::CoreInferred { scope, .. } => {
            Some(*scope)
        }
        SemanticActual::Life(_) => None,
    }
}
fn dependencies(instance: &SemanticInstance, left: &mut usize) -> R<Vec<SemanticInstanceId>> {
    pay(
        left,
        instance
            .actuals
            .len()
            .saturating_add(
                instance
                    .environment
                    .as_ref()
                    .map_or(0, |env| env.actuals.len()),
            )
            .saturating_add(4),
    )?;
    let mut result = Vec::new();
    result.extend(instance.parent);
    if let SemanticLifetimeInstantiation::Partition { rigid } = instance.lifetime_instantiation {
        result.push(rigid);
    }
    result.extend(instance.self_type.as_ref().and_then(reference));
    result.extend(instance.actuals.iter().filter_map(actual_reference));
    if let Some(environment) = &instance.environment {
        result.extend(environment.actuals.iter().filter_map(actual_reference));
    }
    Ok(result)
}

pub(super) fn publish(
    program: &mut SemanticModule,
    lower: &mut Lower<'_, '_>,
) -> R<BTreeMap<SemanticInstanceId, FunctionId>> {
    let mut input = BTreeMap::new();
    let mut sites = BTreeMap::<SemanticInstanceId, Vec<usize>>::new();
    for (ordinal, instance) in program.instances.iter().enumerate() {
        pay(lower.left, search(input.len()).saturating_add(1))?;
        if input.insert(instance.id, ordinal).is_some() {
            return Err(bad("actual instance identity repeats before relocation"));
        }
    }
    for (ordinal, site) in program.sites.iter().enumerate() {
        pay(lower.left, search(sites.len()).saturating_add(1))?;
        sites.entry(site.instance).or_default().push(ordinal);
    }
    let mut targets = BTreeMap::new();
    let mut pending = BTreeSet::new();
    let mut inactive = BTreeSet::new();
    for instance in &program.instances {
        let SemanticInstanceTarget::Function(target) = &instance.target else {
            continue;
        };
        let origin = Lower::origin(target)?;
        pay(
            lower.left,
            search(lower.ledger.templates.len()).saturating_add(1),
        )?;
        if origin.closure.is_some() || request(instance, lower.left)?.is_some() {
            pending.insert(instance.id);
        } else if lower.ledger.templates.contains(&origin) {
            // This is an explicitly registered unspecialized compiler header,
            // not a conclusion from an absent frame, empty type or sparse SSA.
            inactive.insert(instance.id);
        } else {
            let actual = lower.target(target)?;
            targets.insert(instance.id, actual);
        }
    }
    while !pending.is_empty() {
        let mut progress = false;
        let batch: Vec<_> = pending.iter().copied().collect();
        pay(lower.left, batch.len().saturating_mul(3).saturating_add(1))?;
        for id in batch {
            let instance = &program.instances[input[&id]];
            let (dependency, call) = if let Some(request) = request(instance, lower.left)? {
                (request.0, Some(request.1))
            } else {
                (
                    instance
                        .parent
                        .ok_or_else(|| bad("child emitter request has no parent instance"))?,
                    None,
                )
            };
            pay(
                lower.left,
                search(input.len())
                    .saturating_add(search(inactive.len()))
                    .saturating_add(search(targets.len())),
            )?;
            if !input.contains_key(&dependency) {
                return Err(bad(
                    "emitter request references an absent original instance",
                ));
            }
            if inactive.contains(&dependency) {
                inactive.insert(id);
                pending.remove(&id);
                progress = true;
                continue;
            }
            let Some(parent) = targets.get(&dependency).copied() else {
                continue;
            };
            let actual = if let Some(call) = call {
                let site = source_call(program, dependency, call, lower.left)?;
                let SemanticInstanceTarget::Function(target) = &instance.target else {
                    unreachable!()
                };
                called(lower, parent, site, Lower::origin(target)?)?
            } else {
                created(lower, program, instance, parent)?
            };
            lower.actual(actual)?;
            targets.insert(id, actual);
            pending.remove(&id);
            progress = true;
        }
        if !progress {
            return Err(bad(
                "runtime instance requests have no complete acyclic actual emitter binding",
            ));
        }
    }
    let mut live: BTreeSet<_> = targets.keys().copied().collect();
    let mut queue: Vec<_> = live.iter().copied().collect();
    let mut cursor = 0;
    while cursor < queue.len() {
        pay(lower.left, search(input.len()).saturating_add(1))?;
        let id = queue[cursor];
        cursor += 1;
        let instance = &program.instances[input[&id]];
        let mut refs = dependencies(instance, lower.left)?;
        if let Some(rows) = sites.get(&id) {
            pay(lower.left, rows.len())?;
            for ordinal in rows {
                let action = &program.sites[*ordinal].action;
                match action {
                    SemanticSiteAction::Construct { value, .. }
                    | SemanticSiteAction::TypeUse {
                        expected: value, ..
                    }
                    | SemanticSiteAction::DeclareLocal {
                        expected: value, ..
                    } => refs.extend(reference(value)),
                    SemanticSiteAction::Call { actuals, .. }
                    | SemanticSiteAction::BodyCall { actuals }
                    | SemanticSiteAction::Closure {
                        environment: actuals,
                        ..
                    } => {
                        pay(lower.left, actuals.len())?;
                        refs.extend(actuals.iter().filter_map(actual_reference));
                    }
                    SemanticSiteAction::CallableCall { .. } | SemanticSiteAction::CoreBodyCall => {}
                }
            }
        }
        for reference in refs {
            pay(
                lower.left,
                search(input.len())
                    .saturating_add(search(live.len()))
                    .saturating_add(search(inactive.len())),
            )?;
            let target = *input
                .get(&reference)
                .ok_or_else(|| bad("live emission refers to an absent lexical instance"))?;
            if matches!(
                program.instances[target].target,
                SemanticInstanceTarget::Function(_)
            ) && !targets.contains_key(&reference)
            {
                return Err(bad(
                    "live emission depends on an uninstantiated function or closure",
                ));
            }
            if live.insert(reference) {
                queue.push(reference);
            }
        }
    }
    let covered: BTreeSet<_> = targets.values().copied().collect();
    for (function, frame) in &lower.ledger.frames {
        pay(lower.left, search(covered.len()).saturating_add(1))?;
        lower.actual(*function)?;
        if !covered.contains(function) {
            return Err(bad(
                "actual emitted function has no original runtime instance",
            ));
        }
        if let Some((parent, _)) = frame.parent {
            if !covered.contains(&parent) {
                return Err(bad("actual child emitter lost its represented creator"));
            }
        }
    }
    pay(
        lower.left,
        program
            .instances
            .len()
            .saturating_add(program.sites.len())
            .saturating_mul(search(live.len())),
    )?;
    program
        .instances
        .retain(|instance| live.contains(&instance.id));
    program.sites.retain(|site| live.contains(&site.instance));
    for instance in &mut program.instances {
        if let Some(function) = targets.get(&instance.id).copied() {
            instance.target =
                SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function));
            if let Some(environment) = &mut instance.environment {
                environment.closure =
                    SemanticClosureTarget::Typed(lower.closure_for_entry(function)?);
            }
        }
    }
    Ok(targets)
}
