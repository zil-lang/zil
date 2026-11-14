//! Actual retained-helper requests arise at Types' real call emission, rather
//! than at the Source generic expander. Instantiate only that exact original
//! template, retaining its complete child inventory and original definitions.
use super::*;

fn remap(id: &mut SemanticInstanceId, ids: &BTreeMap<SemanticInstanceId, SemanticInstanceId>) {
    if let Some(new) = ids.get(id) {
        *id = *new;
    }
}
fn value(value: &mut SemanticTypeValue, ids: &BTreeMap<SemanticInstanceId, SemanticInstanceId>) {
    if let SemanticTypeValue::Core { scope, .. } = value {
        remap(scope, ids);
    }
}
fn actual(actual: &mut SemanticActual, ids: &BTreeMap<SemanticInstanceId, SemanticInstanceId>) {
    match actual {
        SemanticActual::Type(v) | SemanticActual::Const { ty: v, .. } => value(v, ids),
        SemanticActual::CoreRecipe { scope, .. } | SemanticActual::CoreInferred { scope, .. } => {
            remap(scope, ids)
        }
        SemanticActual::Life(_) => {}
    }
}
fn instance(row: &mut SemanticInstance, ids: &BTreeMap<SemanticInstanceId, SemanticInstanceId>) {
    remap(&mut row.id, ids);
    if let Some(parent) = &mut row.parent {
        remap(parent, ids);
    }
    if let SemanticLifetimeInstantiation::Partition { rigid } = &mut row.lifetime_instantiation {
        remap(rigid, ids);
    }
    if let Some(v) = &mut row.self_type {
        value(v, ids);
    }
    for v in &mut row.actuals {
        actual(v, ids);
    }
    if let Some(env) = &mut row.environment {
        for v in &mut env.actuals {
            actual(v, ids);
        }
    }
}
fn site(row: &mut SemanticSite, ids: &BTreeMap<SemanticInstanceId, SemanticInstanceId>) {
    remap(&mut row.instance, ids);
    match &mut row.action {
        SemanticSiteAction::Construct { value: v, .. }
        | SemanticSiteAction::TypeUse { expected: v, .. }
        | SemanticSiteAction::DeclareLocal { expected: v, .. } => value(v, ids),
        SemanticSiteAction::Call { actuals, .. }
        | SemanticSiteAction::BodyCall { actuals }
        | SemanticSiteAction::Closure {
            environment: actuals,
            ..
        } => {
            for v in actuals {
                actual(v, ids);
            }
        }
        SemanticSiteAction::CallableCall { .. } | SemanticSiteAction::CoreBodyCall => {}
    }
}

pub(super) fn activate(
    program: &mut SemanticModule,
    lower: &mut Lower<'_, '_>,
    targets: &BTreeMap<SemanticInstanceId, FunctionId>,
    seen: &mut BTreeSet<(SemanticInstanceId, SemanticSiteId, FunctionId)>,
) -> R<Vec<SemanticInstanceId>> {
    let mut requests = Vec::new();
    pay(lower.left, program.sites.len())?;
    // Read the real parent frame's directly attributed instructions. A Call's
    // source token and actual callee origin jointly select the request.
    for mint in &program.sites {
        let Some(caller) = targets.get(&mint.instance).copied() else {
            continue;
        };
        if !matches!(
            mint.action,
            SemanticSiteAction::CoreBodyCall | SemanticSiteAction::Call { .. }
        ) {
            continue;
        }
        let SemanticSiteAddress::Core {
            expression,
            occurrence: 0,
            role: SemanticEmissionRole::Call,
        } = mint.at
        else {
            continue;
        };
        let receipt = lower.expression(caller, expression)?;
        pay(lower.left, receipt.instructions.len())?;
        for point in &receipt.instructions {
            let callee = match lower.instruction(caller, *point)?.kind {
                InstructionKind::Call { function, .. }
                | InstructionKind::SuspendCall { function, .. }
                | InstructionKind::ForeignCall { function, .. } => function,
                _ => continue,
            };
            let origin = lower.frame(callee)?.origin;
            pay(lower.left, search(lower.ledger.templates.len()))?;
            if !lower.ledger.templates.contains(&origin) {
                continue;
            }
            // Already Source-requested concrete targets are not minted twice.
            pay(lower.left, targets.len())?;
            if targets.values().any(|f| *f == callee) {
                continue;
            }
            pay(lower.left, program.instances.len())?;
            let mut source_requested = false;
            for row in &program.instances {
                let SemanticInstanceTarget::Function(target) = &row.target else {
                    continue;
                };
                if Lower::origin(target)? == origin
                    && request(row, lower.left)? == Some((mint.instance, mint.recipe))
                {
                    source_requested = true;
                }
            }
            if source_requested {
                continue;
            }
            pay(
                lower.left,
                search(seen.len()).saturating_add(program.instances.len()),
            )?;
            if !seen.insert((mint.instance, mint.recipe, callee)) {
                continue;
            }
            let mut prototype = None;
            for row in &program.instances {
                let SemanticInstanceTarget::Function(target) = &row.target else {
                    continue;
                };
                if Lower::origin(target)? == origin && request(row, lower.left)?.is_none() {
                    if prototype.replace(row.id).is_some() {
                        return Err(bad(
                            "retained helper has ambiguous original template instances",
                        ));
                    }
                }
            }
            let root = prototype
                .ok_or_else(|| bad("actual retained helper has no original template instance"))?;
            requests.push((root, mint.instance, mint.recipe));
        }
    }
    let mut output = Vec::new();
    for (root, caller, call) in requests {
        pay(
            lower.left,
            program
                .instances
                .len()
                .saturating_add(program.definitions.len()),
        )?;
        let source = program
            .instances
            .iter()
            .find(|r| r.id == root)
            .ok_or_else(|| bad("retained root disappeared"))?;
        let def = program
            .definitions
            .iter()
            .find(|r| r.id == source.definition)
            .ok_or_else(|| bad("retained root lost its original definition"))?;
        let count = def.parameters.len();
        let mut descendants = BTreeSet::from([root]);
        loop {
            let n = descendants.len();
            pay(lower.left, program.instances.len())?;
            for row in &program.instances {
                if row.parent.is_some_and(|p| descendants.contains(&p)) {
                    descendants.insert(row.id);
                }
            }
            if n == descendants.len() {
                break;
            }
        }
        let mut ids = BTreeMap::<SemanticInstanceId, SemanticInstanceId>::new();
        for id in &descendants {
            pay(lower.left, program.instances.len())?;
            let next = program
                .instances
                .iter()
                .filter(|r| r.id.module == id.module)
                .map(|r| r.id.index)
                .chain(ids.values().filter_map(|r: &SemanticInstanceId| {
                    (r.module == id.module).then_some(r.index)
                }))
                .max()
                .unwrap_or(0)
                .checked_add(1)
                .ok_or_else(|| bad("retained instance namespace exhausted"))?;
            ids.insert(
                *id,
                SemanticInstanceId {
                    module: id.module,
                    index: next,
                },
            );
        }
        let mut rows = Vec::new();
        pay(lower.left, program.instances.len())?;
        for original in &program.instances {
            if !descendants.contains(&original.id) {
                continue;
            }
            crate::dyn_metadata::semantic_operand_snapshot(original, lower.limits, lower.left, 1)?;
            let mut row = original.clone();
            instance(&mut row, &ids);
            if original.id == root {
                pay(lower.left, count.saturating_add(1))?;
                row.actuals = (0..count)
                    .map(|formal| {
                        Ok(SemanticActual::CoreInferred {
                            scope: caller,
                            call,
                            formal: u32::try_from(formal)
                                .map_err(|_| bad("retained formal ordinal overflow"))?,
                        })
                    })
                    .collect::<R<Vec<_>>>()?;
            }
            output.push(row.id);
            rows.push(row);
        }
        let mut mints = Vec::new();
        pay(lower.left, program.sites.len())?;
        for original in &program.sites {
            if !descendants.contains(&original.instance) {
                continue;
            }
            crate::dyn_metadata::semantic_operand_snapshot(original, lower.limits, lower.left, 1)?;
            let mut row = original.clone();
            site(&mut row, &ids);
            mints.push(row);
        }
        if program.instances.len().saturating_add(rows.len()) > lower.limits.max_nodes
            || program.sites.len().saturating_add(mints.len()) > lower.limits.max_nodes
        {
            return Err(Diagnostic::limit(
                Phase::Types,
                "retained actual emission inventory exceeded",
                None,
            ));
        }
        program.instances.extend(rows);
        program.sites.extend(mints);
    }
    Ok(output)
}
