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
    budget.lineage_push(output, value, Span::default())
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
    struct Prototype {
        base: SemanticInstanceId,
        count: usize,
        descendants: BTreeSet<SemanticInstanceId>,
        instances: Vec<SemanticInstance>,
        sites: Vec<SemanticSite>,
        calls: Vec<Request>,
        used: bool,
    }
    let mut prototypes = Vec::new();
    let mut roots = BTreeMap::new();
    let mut owned = BTreeSet::new();
    for (base, count) in pending {
        let mut descendants = BTreeSet::new();
        budget.lineage_bytes(12 * std::mem::size_of::<SemanticInstanceId>(), span)?;
        descendants.insert(*base);
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
        let mut instances = Vec::new();
        let mut sites = Vec::new();
        let mut calls = Vec::new();
        for (_, module) in modules.iter() {
            for row in &module.instances {
                budget.node(span, 0)?;
                if descendants.contains(&row.id) {
                    prepay_recipe(row, budget, span, 0)?;
                    push(&mut instances, row.clone(), budget)?;
                }
            }
            for row in &module.sites {
                budget.node(span, 0)?;
                if descendants.contains(&row.instance) {
                    prepay_recipe(row, budget, span, 0)?;
                    push(&mut sites, row.clone(), budget)?;
                }
            }
        }
        for request in requests {
            budget.node(span, 0)?;
            if descendants.contains(&request.caller) {
                push(&mut calls, *request, budget)?;
            }
        }
        for id in &descendants {
            budget.lineage_bytes(12 * std::mem::size_of::<SemanticInstanceId>(), span)?;
            owned.insert(*id);
        }
        budget.lineage_bytes(
            12 * std::mem::size_of::<(SemanticInstanceId, usize)>(),
            span,
        )?;
        if roots.insert(*base, prototypes.len()).is_some() {
            return Err(error(span, "duplicate actual specialization request root"));
        }
        push(
            &mut prototypes,
            Prototype {
                base: *base,
                count: *count,
                descendants,
                instances,
                sites,
                calls,
                used: false,
            },
            budget,
        )?;
    }
    struct Activation {
        request: Request,
        ancestry: Vec<SemanticInstanceId>,
    }
    let mut queue = Vec::new();
    for request in requests {
        budget.node(span, 0)?;
        if !owned.contains(&request.caller) {
            push(
                &mut queue,
                Activation {
                    request: *request,
                    ancestry: Vec::new(),
                },
                budget,
            )?;
        }
    }
    let mut seen = BTreeSet::new();
    let mut cursor = 0;
    while cursor < queue.len() {
        budget.node(span, 0)?;
        let activation = &queue[cursor];
        let request = activation.request;
        let index = *roots.get(&request.callee).ok_or_else(|| {
            error(
                span,
                "original call request has no actual specialization root",
            )
        })?;
        let prototype = &mut prototypes[index];
        if activation.ancestry.contains(&prototype.base) {
            return Err(unsupported(
                span,
                "cyclic actual specialization request graph",
            ));
        }
        budget.lineage_bytes(
            12 * std::mem::size_of::<(SemanticInstanceId, SemanticSiteId, SemanticInstanceId)>(),
            span,
        )?;
        if !seen.insert((request.caller, request.call, request.callee)) {
            cursor += 1;
            continue;
        }
        budget.lineage_bytes(
            activation
                .ancestry
                .len()
                .checked_mul(std::mem::size_of::<SemanticInstanceId>())
                .ok_or_else(|| error(span, "specialization ancestry allocation overflow"))?,
            span,
        )?;
        let mut ancestry = activation.ancestry.clone();
        push(&mut ancestry, prototype.base, budget)?;
        let mut map = BTreeMap::new();
        for old in &prototype.descendants {
            let id = if !prototype.used {
                *old
            } else {
                let id = SemanticInstanceId {
                    module: old.module,
                    index: *next,
                };
                *next = next
                    .checked_add(1)
                    .ok_or_else(|| error(span, "call request instance namespace exhausted"))?;
                id
            };
            budget.lineage_bytes(
                12 * std::mem::size_of::<(SemanticInstanceId, SemanticInstanceId)>(),
                span,
            )?;
            map.insert(*old, id);
        }
        if !prototype.used {
            let row = modules
                .iter_mut()
                .flat_map(|(_, m)| &mut m.instances)
                .find(|row| row.id == prototype.base)
                .ok_or_else(|| error(span, "actual call target has no Source instance"))?;
            row.actuals = inferred(request, prototype.count, budget)?;
        } else {
            for original in &prototype.instances {
                prepay_recipe(original, budget, span, 0)?;
                let mut row = original.clone();
                instance(&mut row, &map);
                if original.id == prototype.base {
                    row.actuals = inferred(request, prototype.count, budget)?;
                }
                let module = modules
                    .iter_mut()
                    .find(|(id, _)| *id == row.id.module)
                    .ok_or_else(|| error(span, "call request target module is absent"))?;
                push(&mut module.1.instances, row, budget)?;
            }
            for original in &prototype.sites {
                prepay_recipe(original, budget, span, 0)?;
                let mut row = original.clone();
                site(&mut row, &map);
                let module = modules
                    .iter_mut()
                    .find(|(id, _)| *id == row.instance.module)
                    .ok_or_else(|| error(span, "call request operation module is absent"))?;
                push(&mut module.1.sites, row, budget)?;
            }
        }
        prototype.used = true;
        for dependency in &prototype.calls {
            let caller = *map
                .get(&dependency.caller)
                .ok_or_else(|| error(span, "nested call request lost its actual caller owner"))?;
            budget.lineage_bytes(
                ancestry
                    .len()
                    .checked_mul(std::mem::size_of::<SemanticInstanceId>())
                    .ok_or_else(|| error(span, "nested specialization ancestry overflow"))?,
                span,
            )?;
            push(
                &mut queue,
                Activation {
                    request: Request {
                        caller,
                        call: dependency.call,
                        callee: dependency.callee,
                    },
                    ancestry: ancestry.clone(),
                },
                budget,
            )?;
        }
        cursor += 1;
    }
    if prototypes.iter().any(|row| !row.used) {
        return Err(unsupported(
            span,
            "specialized Core function or lexical scope has no actual original call request",
        ));
    }
    Ok(())
}
