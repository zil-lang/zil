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

fn reserve_rows<T>(rows: &mut Vec<T>, extra: usize, budget: &mut Budget<'_>) -> R<()> {
    let span = Span::default();
    let required = rows
        .len()
        .checked_add(extra)
        .ok_or_else(|| error(span, "specialization output count overflow"))?;
    if required > rows.capacity() {
        budget.identity_work(rows.len(), span)?;
        budget.lineage_bytes(
            required
                .checked_mul(std::mem::size_of::<T>())
                .ok_or_else(|| error(span, "specialization output allocation overflow"))?,
            span,
        )?;
        rows.try_reserve_exact(extra)
            .map_err(|_| error(span, "specialization output allocation failed"))?;
    }
    Ok(())
}

pub(super) fn expand(
    modules: &mut [(ModuleId, SemanticModule)],
    requests: &[Request],
    pending: &[(SemanticInstanceId, usize)],
    next: &mut u32,
    budget: &mut Budget<'_>,
) -> R<()> {
    let span = Span::default();
    if pending.is_empty() && requests.is_empty() {
        return Ok(());
    }
    // The source rows keep their positions throughout publication. Only a
    // repeated activation needs to copy their complete recipes.
    struct Prototype {
        base: SemanticInstanceId,
        root: (usize, usize),
        count: usize,
        descendants: BTreeSet<SemanticInstanceId>,
        instances: Vec<(usize, usize)>,
        sites: Vec<(usize, usize)>,
        calls: Vec<Request>,
        used: bool,
    }
    let mut prototypes = Vec::new();
    let mut roots = BTreeMap::new();
    let mut owned = BTreeSet::new();
    for (base, count) in pending {
        let mut descendants = BTreeSet::new();
        let mut inherited = BTreeSet::new();
        budget.lineage_bytes(12 * std::mem::size_of::<SemanticInstanceId>(), span)?;
        descendants.insert(*base);
        loop {
            let before = descendants.len();
            for (_, module) in modules.iter() {
                for row in &module.instances {
                    budget.node(span, 0)?;
                    let lexical_child = row
                        .parent
                        .is_some_and(|parent| descendants.contains(&parent));
                    let default_scope = matches!(row.target, SemanticInstanceTarget::Scope)
                        && matches!(&row.self_type, Some(SemanticTypeValue::Core { scope, source: SemanticCoreTypeSource::OriginalRecipe(_) })
                            if *scope != row.id && descendants.contains(scope));
                    if (lexical_child || default_scope) && !descendants.contains(&row.id) {
                        budget
                            .lineage_bytes(12 * std::mem::size_of::<SemanticInstanceId>(), span)?;
                        descendants.insert(row.id);
                        if default_scope
                            || row.parent.is_some_and(|parent| inherited.contains(&parent))
                        {
                            budget.lineage_bytes(
                                12 * std::mem::size_of::<SemanticInstanceId>(),
                                span,
                            )?;
                            inherited.insert(row.id);
                        }
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
        let mut root = None;
        for (module_index, (module_id, module)) in modules.iter().enumerate() {
            for (index, row) in module.instances.iter().enumerate() {
                budget.node(span, 0)?;
                if descendants.contains(&row.id) {
                    if row.id.module != *module_id {
                        return Err(error(span, "specialization prototype has another owner module"));
                    }
                    if row.id == *base && root.replace((module_index, index)).is_some() {
                        return Err(error(span, "actual specialization prototype is ambiguous"));
                    }
                    push(&mut instances, (module_index, index), budget)?;
                }
            }
            for (index, row) in module.sites.iter().enumerate() {
                budget.node(span, 0)?;
                if descendants.contains(&row.instance) {
                    if row.instance.module != *module_id {
                        return Err(error(span, "specialization operation has another owner module"));
                    }
                    push(&mut sites, (module_index, index), budget)?;
                }
            }
        }
        for request in requests {
            budget.node(span, 0)?;
            if descendants.contains(&request.caller)
                && !(request.callee == *base && inherited.contains(&request.caller))
            {
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
                root: root.ok_or_else(|| error(span, "actual call target has no Source instance"))?,
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
        parent: Option<usize>,
    }
    struct Publication {
        request: Request,
        prototype: usize,
        copies: Option<BTreeMap<SemanticInstanceId, SemanticInstanceId>>,
    }
    let mut queue = Vec::new();
    for request in requests {
        budget.node(span, 0)?;
        if !owned.contains(&request.caller) {
            push(
                &mut queue,
                Activation {
                    request: *request,
                    parent: None,
                },
                budget,
            )?;
        }
    }
    let mut seen = BTreeSet::new();
    let mut publications = Vec::new();
    let mut extra_rows = Vec::new();
    reserve_rows(&mut extra_rows, modules.len(), budget)?;
    for _ in modules.iter() {
        budget.node(span, 0)?;
        extra_rows.push((0usize, 0usize));
    }
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
        let mut ancestor = activation.parent;
        while let Some(index) = ancestor {
            budget.identity_work(1, span)?;
            if queue[index].request.callee == prototype.base {
                return Err(unsupported(
                    span,
                    "cyclic actual specialization request graph",
                ));
            }
            ancestor = queue[index].parent;
        }
        budget.lineage_bytes(
            12 * std::mem::size_of::<(SemanticInstanceId, SemanticSiteId, SemanticInstanceId)>(),
            span,
        )?;
        if !seen.insert((request.caller, request.call, request.callee)) {
            cursor += 1;
            continue;
        }
        let copies = if prototype.used {
            let mut map = BTreeMap::new();
            for old in &prototype.descendants {
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
            for (module, _) in &prototype.instances {
                budget.identity_work(1, span)?;
                extra_rows[*module].0 = extra_rows[*module].0.checked_add(1)
                    .ok_or_else(|| error(span, "specialization instance count overflow"))?;
            }
            for (module, _) in &prototype.sites {
                budget.identity_work(1, span)?;
                extra_rows[*module].1 = extra_rows[*module].1.checked_add(1)
                    .ok_or_else(|| error(span, "specialization operation count overflow"))?;
            }
            Some(map)
        } else {
            None
        };
        prototype.used = true;
        for dependency in &prototype.calls {
            let caller = match &copies {
                Some(map) => *map.get(&dependency.caller)
                    .ok_or_else(|| error(span, "nested call request lost its actual caller owner"))?,
                None => dependency.caller,
            };
            push(
                &mut queue,
                Activation {
                    request: Request {
                        caller,
                        call: dependency.call,
                        callee: dependency.callee,
                    },
                    parent: Some(cursor),
                },
                budget,
            )?;
        }
        push(
            &mut publications,
            Publication { request, prototype: index, copies },
            budget,
        )?;
        cursor += 1;
    }
    if prototypes.iter().any(|row| !row.used) {
        return Err(unsupported(
            span,
            "specialized Core function or lexical scope has no actual original call request",
        ));
    }
    for ((_, module), (instances, sites)) in modules.iter_mut().zip(extra_rows) {
        reserve_rows(&mut module.instances, instances, budget)?;
        reserve_rows(&mut module.sites, sites, budget)?;
    }
    for publication in &publications {
        budget.node(span, 0)?;
        let Some(map) = &publication.copies else {
            continue;
        };
        let prototype = &prototypes[publication.prototype];
        for &(module, index) in &prototype.instances {
            let original = &modules[module].1.instances[index];
            prepay_recipe(original, budget, span, 0)?;
            let mut row = original.clone();
            instance(&mut row, map);
            if original.id == prototype.base {
                row.actuals = inferred(publication.request, prototype.count, budget)?;
            }
            budget.node(span, 0)?;
            modules[module].1.instances.push(row);
        }
        for &(module, index) in &prototype.sites {
            let original = &modules[module].1.sites[index];
            prepay_recipe(original, budget, span, 0)?;
            let mut row = original.clone();
            site(&mut row, map);
            budget.node(span, 0)?;
            modules[module].1.sites.push(row);
        }
    }
    // The first activation keeps the actual root identity. Publish these
    // substitutions after copying so no clone reads another request's actuals.
    for publication in publications {
        budget.node(span, 0)?;
        if publication.copies.is_none() {
            let prototype = &prototypes[publication.prototype];
            let (module, index) = prototype.root;
            modules[module].1.instances[index].actuals =
                inferred(publication.request, prototype.count, budget)?;
        }
    }
    Ok(())
}
