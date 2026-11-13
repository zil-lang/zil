//! Per-call Core instantiation requests. These are transport proposals only:
//! Types must solve the original caller body before selecting a Typed entry.
use super::*;
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Copy)]
pub(super) struct Request {
    pub caller: SemanticInstanceId,
    pub call: SemanticSiteId,
    pub callee: SemanticInstanceId,
}
fn push<T>(output: &mut Vec<T>, value: T, budget: &mut Budget<'_>) -> R<()> {
    let span = Span::default();
    budget.node(span, 0)?;
    if output.len() == output.capacity() {
        let growth = output.capacity().max(4);
        budget.identity_work(output.len(), span)?;
        budget.lineage_bytes(
            growth
                .checked_mul(std::mem::size_of::<T>())
                .ok_or_else(|| error(span, "call request transport inventory overflow"))?,
            span,
        )?;
        output
            .try_reserve_exact(growth)
            .map_err(|_| error(span, "call request transport allocation failed"))?;
    }
    output.push(value);
    Ok(())
}
fn remap(id: &mut SemanticInstanceId, map: &BTreeMap<SemanticInstanceId, SemanticInstanceId>) {
    if let Some(actual) = map.get(id) {
        *id = *actual;
    }
}
fn ty(value: &mut SemanticTypeValue, map: &BTreeMap<SemanticInstanceId, SemanticInstanceId>) {
    if let SemanticTypeValue::Core { scope, .. } = value {
        remap(scope, map);
    }
}
fn actual(value: &mut SemanticActual, map: &BTreeMap<SemanticInstanceId, SemanticInstanceId>) {
    match value {
        SemanticActual::Type(value) | SemanticActual::Const { ty: value, .. } => ty(value, map),
        SemanticActual::CoreRecipe { scope, .. } | SemanticActual::CoreInferred { scope, .. } => {
            remap(scope, map)
        }
        SemanticActual::Life(_) => {}
    }
}
fn instance(value: &mut SemanticInstance, map: &BTreeMap<SemanticInstanceId, SemanticInstanceId>) {
    remap(&mut value.id, map);
    if let Some(parent) = &mut value.parent {
        remap(parent, map);
    }
    if let SemanticLifetimeInstantiation::Partition { rigid } = &mut value.lifetime_instantiation {
        remap(rigid, map);
    }
    for value in &mut value.actuals {
        actual(value, map);
    }
    if let Some(environment) = &mut value.environment {
        for value in &mut environment.actuals {
            actual(value, map);
        }
    }
    if let Some(value) = &mut value.self_type {
        ty(value, map);
    }
}
fn site(value: &mut SemanticSite, map: &BTreeMap<SemanticInstanceId, SemanticInstanceId>) {
    remap(&mut value.instance, map);
    match &mut value.action {
        SemanticSiteAction::Construct { value, .. } => ty(value, map),
        SemanticSiteAction::TypeUse { expected, .. }
        | SemanticSiteAction::DeclareLocal { expected, .. } => ty(expected, map),
        SemanticSiteAction::Call { actuals, .. } | SemanticSiteAction::BodyCall { actuals } => {
            for value in actuals {
                actual(value, map);
            }
        }
        SemanticSiteAction::Closure { environment, .. } => {
            for value in environment {
                actual(value, map);
            }
        }
        SemanticSiteAction::CallableCall { .. } | SemanticSiteAction::CoreBodyCall => {}
    }
}
fn inferred(request: Request, count: usize, budget: &mut Budget<'_>) -> R<Vec<SemanticActual>> {
    let span = Span::default();
    let mut output = Vec::new();
    budget.lineage_bytes(
        count
            .checked_mul(std::mem::size_of::<SemanticActual>())
            .ok_or_else(|| error(span, "call request actual inventory overflow"))?,
        span,
    )?;
    output
        .try_reserve_exact(count)
        .map_err(|_| error(span, "call request actual allocation failed"))?;
    for formal in 0..count {
        budget.node(span, 0)?;
        output.push(SemanticActual::CoreInferred {
            scope: request.caller,
            call: request.call,
            formal: u32::try_from(formal)
                .map_err(|_| error(span, "call request formal ordinal overflow"))?,
        });
    }
    Ok(output)
}
pub(super) fn expand(
    modules: &mut [(ModuleId, SemanticModule)],
    requests: &[Request],
    pending: &[(SemanticInstanceId, usize)],
    next: &mut u32,
    budget: &mut Budget<'_>,
) -> R<()> {
    let span = Span::default();
    for (base, count) in pending {
        budget.identity_work(requests.len(), span)?;
        let calls = requests.iter().filter(|request| request.callee == *base);
        let mut calls = calls.peekable();
        if calls.peek().is_none() {
            return Err(unsupported(
                span,
                "specialized Core function has no actual original call request",
            ));
        }
        let mut descendants = BTreeSet::new();
        budget.lineage_bytes(12 * std::mem::size_of::<SemanticInstanceId>(), span)?;
        descendants.insert(*base);
        // Source emits lexical children in creation order. Iterate until every
        // finite actual parent is covered; this is directory reachability, not
        // recursive type or permission inference.
        loop {
            let before = descendants.len();
            for (_, module) in modules.iter() {
                for row in &module.instances {
                    budget.node(span, 0)?;
                    if row
                        .parent
                        .is_some_and(|parent| descendants.contains(&parent))
                        && !descendants.contains(&row.id)
                    {
                        budget
                            .lineage_bytes(12 * std::mem::size_of::<SemanticInstanceId>(), span)?;
                        descendants.insert(row.id);
                    }
                }
            }
            if before == descendants.len() {
                break;
            }
        }
        let mut prototypes = Vec::new();
        let mut original_sites = Vec::new();
        for (_, module) in modules.iter() {
            for row in &module.instances {
                budget.node(span, 0)?;
                if descendants.contains(&row.id) {
                    prepay_recipe(row, budget, span, 0)?;
                    budget.lineage_bytes(std::mem::size_of::<SemanticInstance>(), span)?;
                    push(&mut prototypes, row.clone(), budget)?;
                }
            }
            for row in &module.sites {
                budget.node(span, 0)?;
                if descendants.contains(&row.instance) {
                    prepay_recipe(row, budget, span, 0)?;
                    budget.lineage_bytes(std::mem::size_of::<SemanticSite>(), span)?;
                    push(&mut original_sites, row.clone(), budget)?;
                }
            }
        }
        for (ordinal, request) in calls.enumerate() {
            if ordinal == 0 {
                let row = modules
                    .iter_mut()
                    .flat_map(|(_, module)| &mut module.instances)
                    .find(|row| row.id == *base)
                    .ok_or_else(|| error(span, "actual call target has no Source instance"))?;
                row.actuals = inferred(*request, *count, budget)?;
                continue;
            }
            let mut map = BTreeMap::new();
            for old in &descendants {
                let id = SemanticInstanceId {
                    module: old.module,
                    index: *next,
                };
                *next = next
                    .checked_add(1)
                    .ok_or_else(|| error(span, "call request instance namespace exhausted"))?;
                budget.lineage_bytes(
                    12 * std::mem::size_of::<(SemanticInstanceId, SemanticInstanceId)>(),
                    span,
                )?;
                map.insert(*old, id);
            }
            for prototype in &prototypes {
                prepay_recipe(prototype, budget, span, 0)?;
                let mut row = prototype.clone();
                instance(&mut row, &map);
                if prototype.id == *base {
                    row.actuals = inferred(*request, *count, budget)?;
                }
                let module = modules
                    .iter_mut()
                    .find(|(module, _)| *module == row.id.module)
                    .ok_or_else(|| error(span, "call request target module is absent"))?;
                budget.lineage_bytes(std::mem::size_of::<SemanticInstance>(), span)?;
                push(&mut module.1.instances, row, budget)?;
            }
            for prototype in &original_sites {
                prepay_recipe(prototype, budget, span, 0)?;
                let mut row = prototype.clone();
                site(&mut row, &map);
                let module = modules
                    .iter_mut()
                    .find(|(module, _)| *module == row.instance.module)
                    .ok_or_else(|| error(span, "call request operation module is absent"))?;
                budget.lineage_bytes(std::mem::size_of::<SemanticSite>(), span)?;
                push(&mut module.1.sites, row, budget)?;
            }
        }
    }
    Ok(())
}
