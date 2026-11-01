//! Independently bounded source-only generic Product recipes. These facts never
//! grant ownership, StoreElement, capture, Copy or borrowing authority.
use super::*;
use std::collections::BTreeSet;
use zil_base::{Fingerprint, ModuleId, NodeId};
pub(super) type Family = (ModuleId, NodeId);
pub(super) type Owner = (ModuleId, NodeId);

pub(super) fn spend(left: &mut usize, n: usize, span: Span) -> R<()> {
    *left = left.checked_sub(n).ok_or_else(|| {
        Diagnostic::limit(
            Phase::Types,
            "generic Product identity work exceeded",
            Some(span),
        )
    })?;
    Ok(())
}
fn bad(span: Span, message: &str) -> Diagnostic {
    error("E-PRODUCT-TEMPLATE", span, message)
}

#[derive(Clone, Default)]
pub(super) struct Bindings {
    pub ordinary: Option<String>,
    pub callable: Vec<(u32, String)>,
}
impl Bindings {
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.ordinary
            .iter()
            .map(String::as_str)
            .chain(self.callable.iter().map(|(_, n)| n.as_str()))
    }
}
pub(super) struct Index {
    pub templates: BTreeMap<Family, std::rc::Rc<CoreAggregateTemplate>>,
    pub applications: BTreeMap<
        String,
        (
            ModuleId,
            std::rc::Rc<CoreAggregateApplication>,
            std::rc::Rc<CoreAggregate>,
        ),
    >,
    pub owners: BTreeMap<Owner, Bindings>,
    pub resolved: BTreeMap<(ModuleId, NodeId), Vec<Type>>,
    pub symbolic_names: BTreeMap<String, Owner>,
}
impl Index {
    pub fn new(modules: &[&CoreModule], limits: &Limits, remaining: &mut usize) -> R<Self> {
        let mut result = Self {
            templates: BTreeMap::new(),
            applications: BTreeMap::new(),
            owners: BTreeMap::new(),
            resolved: BTreeMap::new(),
            symbolic_names: BTreeMap::new(),
        };
        if modules
            .iter()
            .all(|m| m.aggregate_templates.is_empty() && m.aggregate_applications.is_empty())
        {
            return Ok(result);
        }
        let span = Span::default();
        spend(remaining, modules.len(), span)?;
        for m in modules {
            spend(remaining, m.functions.len(), span)?;
            for f in &m.functions {
                let cost = f
                    .type_parameters
                    .iter()
                    .fold(1usize, |n, p| n.saturating_add(p.name.len() + 1))
                    .saturating_add(
                        f.callable_parameters
                            .iter()
                            .map(|p| p.generic_name.len() + 1)
                            .sum::<usize>(),
                    )
                    .saturating_add(
                        f.state_parameters
                            .iter()
                            .flat_map(|s| &s.bindings)
                            .map(|p| p.generic_name.len() + 1)
                            .sum::<usize>(),
                    );
                spend(remaining, cost, f.span)?;
                store_templates::validate_header(f)?;
                let mut bindings = Bindings {
                    ordinary: f.type_parameters.first().map(|p| p.name.clone()),
                    callable: Vec::new(),
                };
                for (index, name) in f
                    .callable_parameters
                    .iter()
                    .map(|b| (b.generic_index, &b.generic_name))
                    .chain(
                        f.state_parameters
                            .iter()
                            .flat_map(|s| &s.bindings)
                            .map(|b| (b.generic_index, &b.generic_name)),
                    )
                {
                    if let Some((_, old)) = bindings.callable.iter().find(|(i, _)| *i == index) {
                        if old != name {
                            return Err(bad(f.span, "conflicting callable declaration identity"));
                        }
                    } else {
                        bindings.callable.push((index, name.clone()));
                    }
                }
                if result
                    .owners
                    .insert((m.module_id, f.id), bindings)
                    .is_some()
                {
                    return Err(bad(f.span, "duplicate generic Product owner identity"));
                }
            }
            spend(
                remaining,
                m.aggregate_templates
                    .len()
                    .saturating_add(m.aggregate_applications.len())
                    .saturating_mul(2),
                span,
            )?;
            if m.aggregate_templates
                .len()
                .saturating_add(m.aggregate_applications.len())
                > limits.max_nodes
            {
                return Err(Diagnostic::limit(
                    Phase::Types,
                    "generic Product table size exceeded",
                    None,
                ));
            }
            for t in &m.aggregate_templates {
                let before = *remaining;
                spend(
                    remaining,
                    t.name
                        .len()
                        .saturating_add(t.parameter.name.len())
                        .saturating_add(t.fields.len().saturating_mul(2)),
                    t.span,
                )?;
                if t.name.is_empty()
                    || t.parameter.index != 0
                    || t.parameter.name.is_empty()
                    || t.parameter.bound != CoreTypeBound::StoreElement
                {
                    return Err(bad(
                        t.span,
                        "generic Product requires one index-zero StoreElement formal",
                    ));
                }
                let mut names = BTreeSet::new();
                for f in &t.fields {
                    spend(
                        remaining,
                        f.name
                            .len()
                            .saturating_mul(t.fields.len().saturating_add(1)),
                        f.span,
                    )?;
                    if f.name.is_empty() || !names.insert(&f.name) {
                        return Err(bad(
                            f.span,
                            "generic Product recipe fields must be unique and nonempty",
                        ));
                    }
                    validate_recipe(&f.ty, 0, limits, remaining, f.span)?;
                }
                spend(remaining, before.saturating_sub(*remaining), t.span)?;
                if result
                    .templates
                    .insert((m.module_id, t.id), std::rc::Rc::new(t.clone()))
                    .is_some()
                {
                    return Err(bad(t.span, "duplicate generic Product family identity"));
                }
            }
            let mut applications = BTreeSet::new();
            for a in &m.aggregate_applications {
                spend(
                    remaining,
                    m.aggregates.len().saturating_add(a.arguments.len()),
                    span,
                )?;
                if !applications.insert(a.aggregate) {
                    return Err(bad(span, "duplicate generic Product aggregate application"));
                }
                let mut declared = m.aggregates.iter().filter(|d| d.id == a.aggregate);
                let d = declared.next().ok_or_else(|| {
                    bad(span, "generic Product application has no aggregate header")
                })?;
                if declared.next().is_some()
                    || d.kind != AggregateKind::Package
                    || d.teardown.is_some()
                {
                    return Err(bad(
                        d.span,
                        "generic Product application requires one Package header without teardown",
                    ));
                }
                if a.arguments.len() != 1 {
                    return Err(bad(
                        d.span,
                        "generic Product application needs one complete argument",
                    ));
                }
                for arg in &a.arguments {
                    if let CoreAggregateArgument::Concrete(ty) = arg {
                        type_work(ty, limits, remaining, d.span)?;
                    } else if !matches!(arg, CoreAggregateArgument::Parameter { index: 0 }) {
                        return Err(bad(
                            d.span,
                            "generic Product owner parameter is outside its declaration",
                        ));
                    }
                }
                spend(
                    remaining,
                    d.name.len().saturating_add(d.fields.len()),
                    d.span,
                )?;
                for f in &d.fields {
                    spend(remaining, f.name.len(), f.span)?;
                    type_work(&f.ty, limits, remaining, f.span)?;
                }
                for arg in &a.arguments {
                    if let CoreAggregateArgument::Concrete(ty) = arg {
                        type_work(ty, limits, remaining, d.span)?;
                    }
                }
                if result
                    .applications
                    .insert(
                        d.name.clone(),
                        (
                            m.module_id,
                            std::rc::Rc::new(a.clone()),
                            std::rc::Rc::new(d.clone()),
                        ),
                    )
                    .is_some()
                {
                    return Err(bad(d.span, "duplicate generic Product application name"));
                }
            }
        }
        for (_, a, d) in result.applications.values() {
            if !result
                .templates
                .contains_key(&(a.template_module, a.template))
            {
                return Err(bad(d.span, "generic Product application family is absent"));
            }
            if let Some(o) = &a.owner {
                let bindings = result
                    .owners
                    .get(&(o.module, o.function))
                    .ok_or_else(|| bad(d.span, "generic Product lexical owner is absent"))?;
                if a.arguments
                    .iter()
                    .any(|a| matches!(a, CoreAggregateArgument::Parameter { .. }))
                    && bindings.ordinary.is_none()
                {
                    return Err(bad(
                        d.span,
                        "generic Product argument requires its owner's ordinary declaration",
                    ));
                }
            } else if a
                .arguments
                .iter()
                .any(|a| matches!(a, CoreAggregateArgument::Parameter { .. }))
            {
                return Err(bad(
                    d.span,
                    "generic Product parameter has no lexical owner",
                ));
            }
        }
        for t in result.templates.values() {
            for f in &t.fields {
                result.check_links(&f.ty, f.span, remaining)?;
            }
        }
        Ok(result)
    }
    fn check_links(&self, recipe: &CoreAggregateTypeRecipe, span: Span, left: &mut usize) -> R<()> {
        spend(left, 1, span)?;
        match recipe {
            CoreAggregateTypeRecipe::Apply {
                module,
                template,
                arguments,
            } => {
                if !self.templates.contains_key(&(*module, *template)) || arguments.len() != 1 {
                    return Err(bad(
                        span,
                        "generic Product recipe references an absent family or wrong arity",
                    ));
                }
                for a in arguments {
                    self.check_links(a, span, left)?;
                }
            }
            CoreAggregateTypeRecipe::Store { element } => self.check_links(element, span, left)?,
            _ => {}
        }
        Ok(())
    }
}
pub(super) fn type_work(ty: &Type, limits: &Limits, left: &mut usize, span: Span) -> R<()> {
    let work = inference::type_snapshot_work(ty, span, limits)?;
    spend(left, work, span)
}
fn validate_recipe(
    r: &CoreAggregateTypeRecipe,
    depth: usize,
    limits: &Limits,
    left: &mut usize,
    span: Span,
) -> R<()> {
    spend(left, 1, span)?;
    if depth > limits.max_call_depth {
        return Err(Diagnostic::limit(
            Phase::Types,
            "generic Product recipe depth exceeded",
            Some(span),
        ));
    }
    match r {
        CoreAggregateTypeRecipe::Parameter { index: 0 } => {}
        CoreAggregateTypeRecipe::Parameter { .. } => {
            return Err(bad(
                span,
                "generic Product recipe formal is outside its declaration",
            ));
        }
        CoreAggregateTypeRecipe::Concrete(ty) => type_work(ty, limits, left, span)?,
        CoreAggregateTypeRecipe::Store { element } => {
            validate_recipe(element, depth + 1, limits, left, span)?
        }
        CoreAggregateTypeRecipe::Apply { arguments, .. } => {
            spend(left, arguments.len(), span)?;
            for a in arguments {
                validate_recipe(a, depth + 1, limits, left, span)?;
            }
        }
    }
    Ok(())
}
pub(super) fn free(ty: &Type, names: &Bindings) -> bool {
    match ty {
        Type::Named(n) => names.names().any(|name| name == n),
        Type::Package { fields, .. } | Type::Record { fields, .. } => {
            fields.iter().any(|(_, t)| free(t, names))
        }
        Type::Tuple(ts) => ts.iter().any(|t| free(t, names)),
        Type::Managed(t)
        | Type::Desc(t)
        | Type::Address { inner: t, .. }
        | Type::OwnedStore { element: t }
        | Type::Array { element: t, .. }
        | Type::Slice { element: t }
        | Type::Opaque { hidden: t, .. } => free(t, names),
        Type::ErrorUnion { error, result } => free(error, names) || free(result, names),
        Type::Enum { variants, .. } => variants
            .iter()
            .any(|(_, ts)| ts.iter().any(|t| free(t, names))),
        Type::FunctionPointer { parameters, result }
        | Type::ForeignCallback { parameters, result } => {
            parameters.iter().any(|t| free(t, names)) || free(result, names)
        }
        _ => false,
    }
}
pub(super) fn canonical(
    family: Family,
    args: &[Type],
    owner: Option<Owner>,
    limits: &Limits,
    left: &mut usize,
    span: Span,
) -> R<String> {
    let mut work = 1usize;
    for a in args {
        work = work.saturating_add(inference::type_snapshot_work(a, span, limits)?)
    }
    // Debug's fixed tags/punctuation have a bounded per-type-node expansion.
    spend(left, work.saturating_mul(128), span)?;
    let key = format!("{family:?}:{owner:?}:{args:?}");
    Ok(format!(
        "@module{}:product-template{}:{}",
        family.0.0,
        family.1.0,
        Fingerprint::of(key.as_bytes()).to_hex()
    ))
}

#[derive(Clone)]
struct Origin {
    family: Family,
    arguments: Vec<Type>,
    owner: Option<Owner>,
    bindings: Bindings,
}
#[derive(Default)]
pub(super) struct Origins {
    entries: BTreeMap<String, Origin>,
}
pub(super) type Products = Option<std::rc::Rc<std::cell::RefCell<Origins>>>;

pub(super) fn origins(modules: &[&CoreModule], limits: &Limits, left: &mut usize) -> R<Products> {
    if modules.iter().all(|m| m.aggregate_applications.is_empty()) {
        return Ok(None);
    }
    let index = Index::new(modules, limits, left)?;
    let mut origins = Origins::default();
    for (_, application, header) in index.applications.values() {
        let owner = application.owner.as_ref().map(|o| (o.module, o.function));
        let bindings = owner
            .and_then(|o| index.owners.get(&o))
            .cloned()
            .unwrap_or_default();
        spend(left, application.arguments.len(), header.span)?;
        let mut arguments = Vec::new();
        for arg in &application.arguments {
            let ty = match arg {
                CoreAggregateArgument::Concrete(ty) => {
                    type_work(ty, limits, left, header.span)?;
                    ty.clone()
                }
                CoreAggregateArgument::Parameter { index: 0 } => {
                    Type::Named(bindings.ordinary.clone().ok_or_else(|| {
                        bad(header.span, "generic Product origin has no owner binder")
                    })?)
                }
                _ => return Err(bad(header.span, "generic Product origin binder is invalid")),
            };
            arguments.push(ty);
        }
        let owner = if arguments.iter().any(|a| free(a, &bindings)) {
            owner
        } else {
            None
        };
        let family = (application.template_module, application.template);
        let name = canonical(family, &arguments, owner, limits, left, header.span)?;
        spend(
            left,
            name.len().saturating_add(arguments.len()),
            header.span,
        )?;
        origins.entries.insert(
            name,
            Origin {
                family,
                arguments,
                owner,
                bindings,
            },
        );
    }
    Ok(Some(std::rc::Rc::new(std::cell::RefCell::new(origins))))
}

/// Rebuild the nominal identity from complete real arguments after an ordinary
/// or callable binding. The application owner selects its lexical arguments;
/// only a still-symbolic identity includes owner, never a completed instance.
pub(super) fn rebind_type(
    ty: &mut Type,
    replacements: &[(&str, &Type)],
    products: &Products,
    limits: &Limits,
    span: Span,
    charge: &mut impl FnMut(usize) -> R<()>,
) -> R<()> {
    let Some(products) = products else {
        return Ok(());
    };
    let work = inference::type_snapshot_work(ty, span, limits)?;
    charge(work.saturating_mul(4))?;
    fn visit(
        ty: &mut Type,
        r: &[(&str, &Type)],
        products: &std::rc::Rc<std::cell::RefCell<Origins>>,
        limits: &Limits,
        span: Span,
        charge: &mut impl FnMut(usize) -> R<()>,
        depth: usize,
    ) -> R<()> {
        if depth > limits.max_call_depth {
            return Err(Diagnostic::limit(
                Phase::Types,
                "generic Product specialization depth exceeded",
                Some(span),
            ));
        }
        charge(1)?;
        let old = if let Type::Package { name, .. } = ty {
            charge(name.len().saturating_add(products.borrow().entries.len()))?;
            let view = products.borrow();
            if let Some(origin) = view.entries.get(name) {
                let mut work = 1;
                for a in &origin.arguments {
                    work += inference::type_snapshot_work(a, span, limits)?;
                }
                charge(work.saturating_mul(2))?;
                Some(origin.clone())
            } else {
                None
            }
        } else {
            None
        };
        match ty {
            Type::Named(n) => {
                if let Some((_, actual)) = r.iter().find(|(name, _)| *name == n) {
                    charge(inference::type_snapshot_work(actual, span, limits)?)?;
                    *ty = (*actual).clone();
                }
            }
            Type::Package { fields, .. } | Type::Record { fields, .. } => {
                for (_, t) in fields {
                    visit(t, r, products, limits, span, charge, depth + 1)?;
                }
            }
            Type::Tuple(ts) => {
                for t in ts {
                    visit(t, r, products, limits, span, charge, depth + 1)?;
                }
            }
            Type::Managed(t)
            | Type::Desc(t)
            | Type::Address { inner: t, .. }
            | Type::OwnedStore { element: t }
            | Type::Array { element: t, .. }
            | Type::Slice { element: t }
            | Type::Opaque { hidden: t, .. } => {
                visit(t, r, products, limits, span, charge, depth + 1)?
            }
            Type::ErrorUnion { error, result } => {
                visit(error, r, products, limits, span, charge, depth + 1)?;
                visit(result, r, products, limits, span, charge, depth + 1)?;
            }
            Type::Enum { variants, .. } => {
                for (_, ts) in variants {
                    for t in ts {
                        visit(t, r, products, limits, span, charge, depth + 1)?;
                    }
                }
            }
            Type::FunctionPointer { parameters, result }
            | Type::ForeignCallback { parameters, result } => {
                for t in parameters {
                    visit(t, r, products, limits, span, charge, depth + 1)?;
                }
                visit(result, r, products, limits, span, charge, depth + 1)?;
            }
            _ => {}
        }
        if let Some(mut origin) = old {
            for arg in &mut origin.arguments {
                // Preserve legacy State callable-family naming for real F/G.
                charge(r.len().saturating_mul(2))?;
                let callbacks = r
                    .iter()
                    .filter_map(|(n, t)| {
                        if let Type::Closure(id) = t {
                            Some((*n, *id))
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>();
                if let Some((name, id)) = callbacks
                    .first()
                    .filter(|(name, _)| callbacks.iter().all(|(other, _)| other == name))
                {
                    callable_fields::specialize_type(arg, name, *id);
                } else if !callbacks.is_empty() {
                    callable_fields::specialize_type_all(arg, &callbacks);
                }
                visit(arg, r, products, limits, span, charge, depth + 1)?;
            }
            if !origin.arguments.iter().any(|a| free(a, &origin.bindings)) {
                origin.owner = None;
            }
            let mut allowance = limits.max_analysis_steps;
            let name = canonical(
                origin.family,
                &origin.arguments,
                origin.owner,
                limits,
                &mut allowance,
                span,
            )?;
            charge(limits.max_analysis_steps - allowance)?;
            charge(name.len().saturating_add(products.borrow().entries.len()))?;
            let mut registry = products.borrow_mut();
            if let Some(prior) = registry.entries.get(&name) {
                if prior.family != origin.family
                    || prior.arguments != origin.arguments
                    || prior.owner != origin.owner
                {
                    return Err(bad(span, "generic Product canonical identity collision"));
                }
            } else {
                registry.entries.insert(name.clone(), origin);
            }
            if let Type::Package { name: target, .. } = ty {
                *target = name;
            }
        }
        Ok(())
    }
    visit(ty, replacements, products, limits, span, charge, 0)
}

pub(super) fn rebind_function(
    f: &mut CoreFunction,
    replacements: &[(&str, &Type)],
    products: &Products,
    limits: &Limits,
    span: Span,
    charge: &mut impl FnMut(usize) -> R<()>,
) -> R<()> {
    if products.is_none() {
        return Ok(());
    }
    let mut result = Ok(());
    callable_fields::visit(f, &mut |ty| {
        if result.is_ok() {
            result = rebind_type(ty, replacements, products, limits, span, charge);
        }
    });
    result
}

pub(super) fn metadata_snapshot(core: &CoreModule, limits: &Limits, left: &mut usize) -> R<()> {
    if core.aggregate_templates.is_empty() && core.aggregate_applications.is_empty() {
        return Ok(());
    }
    spend(
        left,
        core.aggregate_templates
            .len()
            .saturating_add(core.aggregate_applications.len()),
        Span::default(),
    )?;
    for t in &core.aggregate_templates {
        spend(
            left,
            t.name
                .len()
                .saturating_add(t.parameter.name.len())
                .saturating_add(t.fields.len()),
            t.span,
        )?;
        for f in &t.fields {
            spend(left, f.name.len(), f.span)?;
            validate_recipe(&f.ty, 0, limits, left, f.span)?;
        }
    }
    for a in &core.aggregate_applications {
        spend(left, a.arguments.len().saturating_add(3), Span::default())?;
        for arg in &a.arguments {
            if let CoreAggregateArgument::Concrete(t) = arg {
                type_work(t, limits, left, Span::default())?;
            }
        }
    }
    Ok(())
}

pub(super) fn rebind_callable_type(
    ty: &mut Type,
    bounds: &[CoreCallableParameter],
    bindings: helpers::Bindings,
    state: &mut closures::State,
    limits: &Limits,
    span: Span,
) -> R<()> {
    if state.products.is_none() {
        return Ok(());
    }
    state.snapshot(bounds.len().saturating_mul(4), span)?;
    let actuals = bounds
        .iter()
        .map(|b| {
            Ok((
                b.generic_name.as_str(),
                Type::Closure(bindings.get(b.generic_index, span)?),
            ))
        })
        .collect::<R<Vec<_>>>()?;
    let replacements = actuals.iter().map(|(n, t)| (*n, t)).collect::<Vec<_>>();
    let products = state.products.clone();
    rebind_type(ty, &replacements, &products, limits, span, &mut |n| {
        state.snapshot(n, span)
    })
}
pub(super) fn rebind_callable_function(
    f: &mut CoreFunction,
    bounds: &[CoreCallableParameter],
    bindings: helpers::Bindings,
    state: &mut closures::State,
    limits: &Limits,
    span: Span,
) -> R<()> {
    if state.products.is_none() {
        return Ok(());
    }
    state.snapshot(bounds.len().saturating_mul(4), span)?;
    let actuals = bounds
        .iter()
        .map(|b| {
            Ok((
                b.generic_name.as_str(),
                Type::Closure(bindings.get(b.generic_index, span)?),
            ))
        })
        .collect::<R<Vec<_>>>()?;
    let replacements = actuals.iter().map(|(n, t)| (*n, t)).collect::<Vec<_>>();
    let products = state.products.clone();
    rebind_function(f, &replacements, &products, limits, span, &mut |n| {
        state.snapshot(n, span)
    })
}
