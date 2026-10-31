//! Bounded, hygienic specialization of ordinary type parameters. Output is AST,
//! not a type certificate: every instantiated body enters the normal pipeline.
use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use zil_base::{CompileResult, Diagnostic, Limits, Phase, Span};
use zil_syntax::ast::*;

type R<T> = Result<T, Diagnostic>;
type Env = BTreeMap<String, Option<TypeRef>>;
#[path = "generic_budget.rs"]
mod budget;
pub(super) use budget::Budget;
#[path = "generic_alias.rs"]
mod aliases;
#[path = "generic_bytes.rs"]
mod bytes;
#[path = "generic_callable.rs"]
pub(super) mod callable;
#[path = "generic_copy.rs"]
mod copy;
#[path = "generic_enum.rs"]
mod enums;
#[path = "generic_helper_graph.rs"]
mod helper_graph;
#[path = "generic_state.rs"]
mod states;
#[path = "generic_store_templates.rs"]
pub(super) mod store_templates;
#[path = "generic_store.rs"]
mod stores;
#[path = "generic_trait_bounds.rs"]
mod trait_bounds;
#[path = "generic_where.rs"]
mod where_copy;

fn check_nominal_name(name: &str, span: Span) -> R<()> {
    if super::byte_buffer_name(name) || super::owned_store_name(name) {
        return Err(super::primitive_name_error(name, span));
    }
    if copy::reserved_name(name) {
        return Err(error(span, "Copy is a reserved structural capability name"));
    }
    Ok(())
}

pub(super) fn check_generated_type_name(name: &str, span: Span) -> R<()> {
    check_nominal_name(name, span)?;
    if aliases::reserved_declaration_name(name) {
        return Err(Diagnostic::error(
            Phase::Expand,
            "E-NAME-DUPLICATE",
            Some(span),
            "duplicate alias or declaration name",
        ));
    }
    Ok(())
}

fn check_nominal_name(name: &str, span: Span) -> R<()> {
    if super::byte_buffer_name(name) || super::owned_store_name(name) {
        return Err(super::primitive_name_error(name, span));
    }
    if copy::reserved_name(name) {
        return Err(error(span, "Copy is a reserved structural capability name"));
    }
    Ok(())
}

pub(super) fn check_generated_type_name(name: &str, span: Span) -> R<()> {
    check_nominal_name(name, span)?;
    if aliases::reserved_declaration_name(name) {
        return Err(Diagnostic::error(
            Phase::Expand,
            "E-NAME-DUPLICATE",
            Some(span),
            "duplicate alias or declaration name",
        ));
    }
    Ok(())
}

pub(super) fn specialize<'a>(
    module: &'a Module,
    limits: &Limits,
) -> CompileResult<Cow<'a, Module>> {
    let mut names_budget = Budget::new(limits);
    let mut has_where = false;
    for item in &module.items {
        names_budget.node(item.span, 0).map_err(|e| vec![e])?;
        has_where |= matches!(&item.kind, ItemKind::Function(f) if !f.where_clause.is_empty());
        if let ItemKind::Impl(implementation) = &item.kind
            && implementation
                .trait_type
                .as_ref()
                .is_some_and(stores::is_bound)
        {
            return Err(vec![error(
                item.span,
                "StoreElement is a compiler shape bound and cannot be implemented",
            )]);
        }
        if let ItemKind::Impl(implementation) = &item.kind
            && implementation
                .trait_type
                .as_ref()
                .is_some_and(copy::is_bound)
        {
            return Err(vec![error(
                item.span,
                "Copy is a compiler structural bound and cannot be implemented",
            )]);
        }
        let declared = match &item.kind {
            ItemKind::Aggregate(a) => Some(a.name.as_str()),
            ItemKind::Enum(e) => Some(e.name.as_str()),
            ItemKind::TypeAlias(a) => Some(a.name.as_str()),
            ItemKind::Trait(t) => Some(t.name.as_str()),
            _ => None,
        };
        if let Some(name) = declared {
            check_nominal_name(name, item.span).map_err(|error| vec![error])?;
        }
        for param in params(item) {
            names_budget.node(param.span, 0).map_err(|e| vec![e])?;
            if super::byte_buffer_name(&param.name) || super::owned_store_name(&param.name) {
                return Err(vec![super::primitive_name_error(&param.name, param.span)]);
            }
        }
    }
    if has_where {
        let normalized = where_copy::normalize(module, &mut names_budget).map_err(|e| vec![e])?;
        let (aliased, budget) =
            aliases::normalize_with_budget(&normalized, names_budget).map_err(|e| vec![e])?;
        return match aliased {
            Some(aliased) => {
                specialize_with_budget(&aliased.module, limits, &aliased.pattern_types, budget)
            }
            None => specialize_with_budget(&normalized, limits, &BTreeMap::new(), budget),
        }
        .map(Cow::Owned)
        .map_err(|e| vec![e]);
    }
    if let Some(normalized) = aliases::normalize(module, limits).map_err(|e| vec![e])? {
        if normalized
            .module
            .items
            .iter()
            .all(|item| params(item).is_empty())
        {
            return Ok(Cow::Owned(normalized.module));
        }
        return specialize_engine(&normalized.module, limits, &normalized.pattern_types)
            .map(Cow::Owned)
            .map_err(|error| vec![error]);
    }
    if module.items.iter().all(|item| params(item).is_empty()) {
        return Ok(Cow::Borrowed(module));
    }
    specialize_engine(module, limits, &BTreeMap::new())
        .map(Cow::Owned)
        .map_err(|error| vec![error])
}

fn specialize_engine(
    module: &Module,
    limits: &Limits,
    pattern_types: &BTreeMap<String, TypeRef>,
) -> R<Module> {
    specialize_with_budget(module, limits, pattern_types, Budget::new(limits))
}

fn specialize_with_budget<'a>(
    module: &'a Module,
    limits: &'a Limits,
    pattern_types: &'a BTreeMap<String, TypeRef>,
    budget: Budget<'a>,
) -> R<Module> {
    let mut engine = Engine::with_budget(module, limits, pattern_types, budget)?;
    if !engine.nominal_binder_collision {
        return engine.run(module);
    }
    // Generated aggregates no longer carry the original generic declaration.
    // Protect actual nominal identities in the original, still-scoped AST,
    // before substituting a caller binder into those aggregate fields.
    let protected = super::modules::protect_alias_nominals(module, &mut engine.budget)?;
    let mut engine = Engine::with_budget(&protected, limits, pattern_types, engine.budget)?;
    if engine.nominal_binder_collision {
        return Err(error(
            module.span,
            "type binder still collides with a nominal identity after scoped normalization",
        ));
    }
    engine.run(&protected)
}

struct Engine<'a> {
    pattern_types: &'a BTreeMap<String, TypeRef>,
    templates: BTreeMap<String, &'a Item>,
    enums: BTreeMap<String, &'a EnumDecl>,
    nominals: BTreeMap<String, &'a Item>,
    traits: trait_bounds::Index<'a>,
    return_type: Option<TypeRef>,
    callable_name: Option<String>,
    callable_pair: Option<[String; 2]>,
    store_parameter: Option<String>,
    retained_stores: Vec<String>,
    inferred_state_values: BTreeSet<String>,
    has_store_templates: bool,
    has_single_state_values: bool,
    nominal_binder_collision: bool,
    signatures: BTreeMap<String, Cow<'a, Function>>,
    foreign_signatures: BTreeMap<&'a str, (&'a str, &'a Function)>,
    instances: BTreeMap<(String, Vec<String>), String>,
    instance_types: BTreeMap<String, TypeRef>,
    state_families: BTreeMap<(String, Vec<Option<String>>), usize>,
    pending: VecDeque<Item>,
    limits: &'a Limits,
    budget: Budget<'a>,
}

fn error(span: Span, text: impl Into<String>) -> Diagnostic {
    Diagnostic::error(Phase::Expand, "E-GENERIC-ARGUMENT", Some(span), text)
}
fn unsupported(span: Span, text: impl Into<String>) -> Diagnostic {
    Diagnostic::unsupported(Phase::Expand, text, Some(span))
}
fn named(name: &str, span: Span) -> TypeRef {
    TypeRef {
        kind: TypeKind::Named {
            path: vec![name.to_owned()],
            args: vec![],
        },
        span,
    }
}
fn params(item: &Item) -> &[GenericParam] {
    match &item.kind {
        ItemKind::Function(f) => &f.generics,
        ItemKind::Aggregate(a) => &a.generics,
        ItemKind::Enum(e) => &e.generics,
        _ => &[],
    }
}
fn name(item: &Item) -> Option<&str> {
    match &item.kind {
        ItemKind::Function(f) => Some(&f.name),
        ItemKind::Aggregate(a) => Some(&a.name),
        ItemKind::Enum(e) => Some(&e.name),
        _ => None,
    }
}

impl<'a> Engine<'a> {
    fn with_budget(
        module: &'a Module,
        limits: &'a Limits,
        pattern_types: &'a BTreeMap<String, TypeRef>,
        mut budget: Budget<'a>,
    ) -> R<Self> {
        let traits = trait_bounds::Index::new(module, &mut budget)?;
        let mut engine = Self {
            pattern_types,
            traits,
            templates: BTreeMap::new(),
            enums: BTreeMap::new(),
            nominals: BTreeMap::new(),
            return_type: None,
            callable_name: None,
            callable_pair: None,
            store_parameter: None,
            retained_stores: Vec::new(),
            inferred_state_values: BTreeSet::new(),
            has_store_templates: false,
            has_single_state_values: false,
            nominal_binder_collision: false,
            signatures: BTreeMap::new(),
            foreign_signatures: BTreeMap::new(),
            instances: BTreeMap::new(),
            instance_types: BTreeMap::new(),
            state_families: BTreeMap::new(),
            pending: VecDeque::new(),
            limits,
            budget,
        };
        let mut source_names = BTreeSet::new();
        for item in &module.items {
            engine.budget.item(item)?;
            if let ItemKind::Function(function) = &item.kind
                && super::primitive_name(&function.name)
            {
                return Err(super::primitive_name_error(&function.name, function.span));
            }
            if let Some(name) = name(item)
                && !source_names.insert(name)
            {
                return Err(error(
                    item.span,
                    format!("duplicate source declaration {name}"),
                ));
            }
            if matches!(item.kind, ItemKind::Enum(_) | ItemKind::Aggregate(_)) {
                engine
                    .nominals
                    .insert(name(item).expect("nominal name").to_owned(), item);
            }
            if let ItemKind::Enum(e) = &item.kind {
                engine.enums.insert(e.name.clone(), e);
            }
            if let ItemKind::Function(f) = &item.kind {
                // The item/header is already prepaid. Only a module with a
                // real ordinary StoreElement template needs the new single-F
                // producer facts; old State-only callers keep their cost.
                engine.has_store_templates |= store_templates::parameter(f).is_some();
                engine.signatures.insert(f.name.clone(), Cow::Borrowed(f));
            }
            if let ItemKind::Extern { abi, function } = &item.kind {
                // Borrow the signature in the existing prepaid item traversal.
                // It is not a normal helper and is never a callable role.
                engine
                    .foreign_signatures
                    .insert(&function.name, (abi, function));
            }
            if !params(item).is_empty() {
                let key = name(item).expect("template has a name").to_owned();
                if engine.templates.insert(key, item).is_some() {
                    return Err(error(item.span, "duplicate generic declaration"));
                }
            }
        }
        // Validate the parametric subset even when a template is not instantiated.
        let mut helper_dependencies = helper_graph::Dependencies::default();
        for item in engine.templates.values() {
            if !item.attributes.is_empty() {
                return Err(unsupported(
                    item.span,
                    "unexecuted generic declaration attribute",
                ));
            }
            let callable = if let ItemKind::Function(function) = &item.kind {
                callable::template(function)?
            } else {
                None
            };
            let mut seen = BTreeSet::new();
            for parameter in params(item) {
                // This parameter/name was already charged by item preflight.
                // Borrow the existing nominal index; do not add a global scan,
                // name copy or a hygiene pass to the noncolliding old path.
                if engine.nominals.contains_key(&parameter.name) {
                    if callable
                        .as_ref()
                        .is_some_and(|bounds| bounds.contains_generic(&parameter.name))
                    {
                        // Preserve the already-accounted callable protection path.
                        engine.nominal_binder_collision = true;
                    } else if matches!(parameter.kind, GenericParamKind::Type(_)) {
                        // Ordinary type binders need the same nominal isolation.
                        // Do not inspect a symbolic body against an ambiguously
                        // named nominal table: the caller first protects the
                        // fully prepaid AST, then validates that scoped module.
                        engine.nominal_binder_collision = true;
                        return Ok(engine);
                    }
                }
                if !seen.insert(&parameter.name) {
                    return Err(error(parameter.span, "duplicate type parameter"));
                }
                match &parameter.kind {
                    GenericParamKind::Type(bounds)
                        if bounds.is_empty()
                            || callable.is_some()
                            || bounds.iter().all(stores::is_bound)
                            || matches!(item.kind, ItemKind::Function(_)) => {}
                    _ => {
                        return Err(unsupported(
                            parameter.span,
                            "generic bound, lifetime or const parameter requires its semantic solver",
                        ));
                    }
                }
            }
            if let ItemKind::Function(f) = &item.kind {
                if f.params.iter().any(|p| !p.attributes.is_empty()) {
                    return Err(unsupported(
                        f.span,
                        "unexecuted generic parameter attribute",
                    ));
                }
                if !f.where_clause.is_empty() {
                    return Err(unsupported(f.span, "generic where/contract solving"));
                }
                stores::result_mode(f)?;
                validate_parametric(
                    f,
                    check::ParametricTables {
                        nominals: &engine.nominals,
                        enums: &engine.enums,
                        signatures: &engine.signatures,
                        foreign_signatures: &engine.foreign_signatures,
                        pattern_types: engine.pattern_types,
                        traits: &engine.traits,
                    },
                    &mut helper_dependencies,
                    &mut engine.budget,
                )?;
            }
            if let ItemKind::Enum(e) = &item.kind {
                enums::validate_declaration(e, item.span, &engine.nominals, &mut engine.budget)?;
            }
            if let ItemKind::Aggregate(a) = &item.kind {
                let variables: BTreeSet<_> = a.generics.iter().map(|p| p.name.clone()).collect();
                let bounds = engine
                    .traits
                    .parameter_bounds(&a.generics, &mut engine.budget)?;
                for field in &a.fields {
                    if field.manages
                        && stores::element(&field.ty).is_none()
                        && variables
                            .iter()
                            .any(|variable| callable::refers(&field.ty, variable))
                    {
                        return Err(error(
                            field.span,
                            "a generic manages field needs an explicit resource category; StoreElement is a Value bound",
                        ));
                    }
                    engine.traits.store_types(
                        &field.ty,
                        &bounds,
                        &variables,
                        &mut engine.budget,
                        callable::StoreCallbacks::None,
                    )?;
                    super::foreign::callback_type_tree(
                        &field.ty,
                        &variables,
                        &mut engine.budget,
                        0,
                    )?;
                }
                if a.fields.iter().any(|field| field.default.is_some()) {
                    return Err(unsupported(
                        item.span,
                        "generic field defaults require symbolic default checking",
                    ));
                }
                if a.fields.iter().any(|field| !field.attributes.is_empty()) {
                    return Err(unsupported(item.span, "unexecuted generic field attribute"));
                }
                if !matches!(a.kind, AggregateKind::Struct | AggregateKind::Package)
                    || !a.methods.is_empty()
                {
                    return Err(unsupported(
                        item.span,
                        "generic resource or methods require capability resolution",
                    ));
                }
            }
        }
        helper_dependencies.check(&mut engine.budget)?;
        Ok(engine)
    }
    fn tick(&mut self, span: Span, depth: usize) -> R<()> {
        self.budget.node(span, depth)
    }
    fn run(&mut self, module: &Module) -> R<Module> {
        let mut result = Module {
            id: module.id,
            span: module.span,
            items: vec![],
        };
        for item in &module.items {
            if params(item).is_empty()
                || matches!(&item.kind, ItemKind::Function(f)
                    if callable::template(f)?.is_some_and(|bounds| f.generics.iter().all(|p| bounds.contains_generic(&p.name))))
            {
                self.budget.item(item)?;
                self.pending.push_back(item.clone());
            }
        }
        while let Some(mut item) = self.pending.pop_front() {
            self.item(&mut item)?;
            result.items.push(item);
        }
        Ok(result)
    }
    fn instantiate(&mut self, template: &str, arguments: Vec<TypeRef>, span: Span) -> R<String> {
        match self.instantiate_inner(template, arguments, span, false)? {
            store_templates::Instantiation::Eager(name) => Ok(name),
            store_templates::Instantiation::Deferred { .. } => {
                unreachable!("only an ordinary direct call can retain a StoreElement template")
            }
        }
    }
    fn instantiate_inner(
        &mut self,
        template: &str,
        arguments: Vec<TypeRef>,
        span: Span,
        allow_store_template: bool,
    ) -> R<store_templates::Instantiation> {
        let item = self
            .templates
            .get(template)
            .cloned()
            .ok_or_else(|| error(span, format!("unknown generic declaration {template}")))?;
        // Canonical source type applications identify an instance, independent
        // of whether inference or explicit ::<...> reached it first.
        let scope = if allow_store_template {
            self.store_parameter.as_deref().map_or_else(
                || {
                    callable::StoreCallbacks::from_names(
                        self.callable_name.as_deref(),
                        self.callable_pair.as_ref(),
                    )
                },
                callable::StoreCallbacks::Single,
            )
        } else {
            callable::StoreCallbacks::None
        };
        let mut dependent = false;
        let arguments = arguments
            .iter()
            .map(|ty| {
                original_type_observed(
                    ty,
                    &self.instance_types,
                    &mut self.budget,
                    0,
                    scope,
                    &mut dependent,
                )
            })
            .collect::<R<Vec<_>>>()?;
        let retained = match &item.kind {
            ItemKind::Function(function) => callable::template(function)?,
            _ => None,
        };
        let parameters: Vec<_> = params(item)
            .iter()
            .filter(|p| {
                retained
                    .as_ref()
                    .is_none_or(|bound| !bound.contains_generic(&p.name))
            })
            .collect();
        if parameters.len() != arguments.len() {
            return Err(error(
                span,
                format!(
                    "{template} requires {} type arguments, got {}",
                    parameters.len(),
                    arguments.len()
                ),
            ));
        }
        if dependent {
            return self.retain_store_template(template, item, arguments, span);
        }
        let mut key = (
            template.to_owned(),
            arguments
                .iter()
                .map(|ty| type_key(ty, &mut self.budget))
                .collect::<R<Vec<_>>>()?,
        );
        let family = self.state_family(item, &arguments, &key.1, span)?;
        if let Some(family) = family {
            // A source type name shadowing a callable binder must not share a
            // symbolic aggregate instance with an ordinary concrete argument.
            self.budget.node(span, 0)?;
            key.1.push(format!("$state-family{family}"));
        }
        if let Some(existing) = self.instances.get(&key) {
            return Ok(store_templates::Instantiation::Eager(existing.clone()));
        }
        if self
            .instances
            .len()
            .saturating_add(self.retained_stores.len())
            >= self.limits.max_functions.min(512)
        {
            return Err(Diagnostic::limit(
                Phase::Expand,
                "generic instance budget exceeded",
                Some(span),
            ));
        }
        let generated = match family {
            Some(family) => format!("$generic_{template}_$data{family}_{}", self.instances.len()),
            None => format!("$generic_{}_{}", template, self.instances.len()),
        };
        self.instances.insert(key, generated.clone());
        if matches!(item.kind, ItemKind::Aggregate(_) | ItemKind::Enum(_)) {
            self.instance_types.insert(
                generated.clone(),
                TypeRef {
                    span,
                    kind: TypeKind::Named {
                        path: vec![template.to_owned()],
                        args: arguments.iter().cloned().map(GenericArg::Type).collect(),
                    },
                },
            );
        }
        let substitution: BTreeMap<_, _> = parameters
            .iter()
            .zip(arguments)
            .map(|(p, ty)| (p.name.clone(), ty))
            .collect();
        self.traits.check_arguments(
            params(item),
            &substitution,
            (
                &trait_bounds::Bounds::new(),
                &BTreeSet::new(),
                callable::StoreCallbacks::from_names(
                    self.callable_name.as_deref(),
                    self.callable_pair.as_ref(),
                ),
            ),
            &mut self.budget,
            span,
            retained.as_ref(),
        )?;
        self.budget.substituted_item(item, &substitution)?;
        let mut instance = item.clone();
        substitute_item(&mut instance, &substitution)?;
        self.budget.item(&instance)?;
        match &mut instance.kind {
            ItemKind::Function(f) => {
                f.generics.retain(|parameter| {
                    retained
                        .as_ref()
                        .is_some_and(|bound| bound.contains_generic(&parameter.name))
                });
                f.name.clone_from(&generated);
                self.signatures
                    .insert(generated.clone(), Cow::Owned(f.clone()));
            }
            ItemKind::Aggregate(a) => {
                a.generics.clear();
                a.name.clone_from(&generated);
            }
            ItemKind::Enum(e) => {
                e.generics.clear();
                e.name.clone_from(&generated);
            }
            _ => unreachable!(),
        }
        self.pending.push_back(instance);
        Ok(store_templates::Instantiation::Eager(generated))
    }
    fn ty(&mut self, ty: &mut TypeRef, depth: usize) -> R<()> {
        self.tick(ty.span, depth)?;
        match &mut ty.kind {
            TypeKind::Named { path, args } => {
                let mut values = Vec::new();
                for arg in args.iter_mut() {
                    let GenericArg::Type(value) = arg else {
                        return Err(unsupported(ty.span, "const generic argument"));
                    };
                    self.ty(value, depth + 1)?;
                    values.push(value.clone());
                }
                if !values.is_empty() && path.len() == 1 && self.templates.contains_key(&path[0]) {
                    if !matches!(
                        self.templates[&path[0]].kind,
                        ItemKind::Aggregate(_) | ItemKind::Enum(_)
                    ) {
                        return Err(error(ty.span, "function template used as a type"));
                    }
                    let instance = self.instantiate(&path[0], values, ty.span)?;
                    *path = vec![instance];
                    args.clear();
                }
            }
            TypeKind::Reference { inner, .. }
            | TypeKind::Group(inner)
            | TypeKind::Optional(inner)
            | TypeKind::ForLifetime { inner, .. } => self.ty(inner, depth + 1)?,
            TypeKind::Tuple(items) | TypeKind::Union(items) | TypeKind::Impl(items) => {
                for item in items {
                    self.ty(item, depth + 1)?;
                }
            }
            TypeKind::Array { element, .. } => self.ty(element, depth + 1)?,
            TypeKind::Function { params, result }
            | TypeKind::ForeignFunction { params, result, .. } => {
                for p in params {
                    self.ty(p, depth + 1)?;
                }
                self.ty(result, depth + 1)?;
            }
            TypeKind::Error { error, result } => {
                self.ty(error, depth + 1)?;
                self.ty(result, depth + 1)?;
            }
            _ => {}
        }
        Ok(())
    }
    fn item(&mut self, item: &mut Item) -> R<()> {
        match &mut item.kind {
            ItemKind::Function(f) => {
                let store_parameter = store_templates::parameter(f);
                if let Some(parameter) = store_parameter {
                    self.budget.node(parameter.span, 0)?;
                    self.budget.text(&parameter.name, parameter.span)?;
                }
                let saved_store = std::mem::replace(
                    &mut self.store_parameter,
                    store_parameter.map(|parameter| parameter.name.clone()),
                );
                let callable = callable::template(f)?;
                let pair = callable::dual_scope(callable.as_ref(), &mut self.budget, f.span)?;
                let saved_pair = std::mem::replace(&mut self.callable_pair, pair);
                let saved_callable = std::mem::replace(
                    &mut self.callable_name,
                    callable
                        .as_ref()
                        .and_then(|bound| bound.single_generic().map(str::to_owned)),
                );
                for parameter in &mut f.generics {
                    if let GenericParamKind::Type(bounds) = &mut parameter.kind {
                        for bound in bounds {
                            if let TypeKind::Named { path, args } = &mut bound.kind
                                && path.len() == 1
                                && matches!(path[0].as_str(), "Fn" | "FnMut" | "FnOnce")
                            {
                                // Keep the callable keyword; specialize only its
                                // argument/result types with the ordinary budget.
                                self.tick(bound.span, 0)?;
                                for argument in args {
                                    let GenericArg::Type(ty) = argument else {
                                        return Err(unsupported(
                                            bound.span,
                                            "const callable argument",
                                        ));
                                    };
                                    self.ty(ty, 1)?;
                                }
                            }
                        }
                    }
                }
                for p in &mut f.params {
                    self.ty(&mut p.ty, 0)?;
                }
                if let Some(t) = &mut f.return_type {
                    self.ty(t, 0)?;
                }
                let mut env: Env = f
                    .params
                    .iter()
                    .map(|p| (p.name.clone(), Some(p.ty.clone())))
                    .collect();
                let saved_result = self
                    .return_type
                    .replace(f.return_type.clone().unwrap_or_else(|| check::unit(f.span)));
                match &mut f.body {
                    FunctionBody::Block(body) => self.block(body, &mut env, 0)?,
                    FunctionBody::Expression(value) => {
                        let expected = self.return_type.clone();
                        self.expr_expected(value, &mut env, expected.as_ref(), 0)?;
                    }
                    FunctionBody::Declaration => {}
                }
                self.return_type = saved_result;
                self.callable_name = saved_callable;
                self.callable_pair = saved_pair;
                self.store_parameter = saved_store;
            }
            ItemKind::Aggregate(a) => {
                for field in &mut a.fields {
                    self.ty(&mut field.ty, 0)?;
                    if let Some(value) = &mut field.default {
                        self.expr(value, &mut Env::new(), 0)?;
                    }
                }
                for item in &mut a.methods {
                    self.item(item)?;
                }
            }
            ItemKind::Enum(e) => {
                for variant in &mut e.variants {
                    match &mut variant.fields {
                        VariantFields::Tuple(fields) => {
                            for field in fields {
                                self.ty(field, 0)?;
                            }
                        }
                        VariantFields::Record(fields) => {
                            for field in fields {
                                self.ty(&mut field.ty, 0)?;
                            }
                        }
                        VariantFields::Unit => {}
                    }
                }
            }
            ItemKind::Impl(i) if i.generics.is_empty() => {
                self.ty(&mut i.target, 0)?;
                for item in &mut i.members {
                    self.item(item)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    fn block(&mut self, block: &mut Block, env: &mut Env, depth: usize) -> R<()> {
        self.tick(block.span, depth)?;
        let mut local = self.clone_env(env)?;
        for statement in &mut block.statements {
            self.statement(statement, &mut local, depth + 1)?;
        }
        Ok(())
    }
    fn condition(&mut self, value: &mut Condition, env: &mut Env, depth: usize) -> R<()> {
        match value {
            Condition::Expression(e) => {
                self.expr(e, env, depth)?;
            }
            Condition::Let { pattern, value } => {
                let ty = self.expr(value, env, depth)?;
                self.enum_pattern(pattern, ty.as_ref(), env, depth + 1)?;
            }
        }
        Ok(())
    }
    fn statement(&mut self, s: &mut Stmt, env: &mut Env, depth: usize) -> R<()> {
        self.tick(s.span, depth)?;
        match &mut s.kind {
            StmtKind::Binding {
                pattern, ty, value, ..
            } => {
                if let Some(t) = ty {
                    self.ty(t, depth + 1)?;
                }
                let inferred = self.expr_expected(value, env, ty.as_ref(), depth + 1)?;
                bind(pattern, ty.clone().or(inferred), env);
            }
            StmtKind::Return(Some(e)) => {
                let expected = self.return_type.clone();
                self.expr_expected(e, env, expected.as_ref(), depth + 1)?;
            }
            StmtKind::Yield(e) | StmtKind::Expression(e) => {
                self.expr(e, env, depth + 1)?;
            }
            StmtKind::Assign { target, value, .. } => {
                let expected = self.expr(target, env, depth + 1)?;
                self.expr_expected(value, env, expected.as_ref(), depth + 1)?;
            }
            StmtKind::If {
                condition,
                then_body,
                else_body,
            } => {
                let mut success = self.clone_env(env)?;
                self.condition(condition, &mut success, depth + 1)?;
                self.block(then_body, &mut success, depth + 1)?;
                match else_body {
                    Some(ElseBranch::Block(b)) => self.block(b, env, depth + 1)?,
                    Some(ElseBranch::If(s)) => self.statement(s, env, depth + 1)?,
                    None => {}
                }
            }
            StmtKind::While { condition, body } => {
                let mut success = self.clone_env(env)?;
                self.condition(condition, &mut success, depth + 1)?;
                self.block(body, &mut success, depth + 1)?;
            }
            StmtKind::For {
                iterable,
                body,
                pattern,
            } => {
                let ty = self.range_expression(iterable, env, depth + 1)?;
                let mut inner = self.clone_env(env)?;
                bind(pattern, ty, &mut inner);
                self.block(body, &mut inner, depth + 1)?;
            }
            StmtKind::Loop(b) | StmtKind::Unsafe(b) | StmtKind::Defer(b) => {
                self.block(b, env, depth + 1)?
            }
            StmtKind::With { value, body, .. } => {
                self.expr(value, env, depth + 1)?;
                self.block(body, env, depth + 1)?;
            }
            StmtKind::Match { value, arms } => {
                let ty = self.expr(value, env, depth + 1)?;
                for arm in arms {
                    let mut inner = self.clone_env(env)?;
                    self.enum_pattern(&mut arm.pattern, ty.as_ref(), &mut inner, depth + 1)?;
                    self.arrow(&mut arm.body, &mut inner, depth + 1)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    fn arrow(&mut self, body: &mut ArrowBody, env: &mut Env, depth: usize) -> R<()> {
        match body {
            ArrowBody::Block(b) => self.block(b, env, depth),
            ArrowBody::Expression(e) => {
                self.expr(e, env, depth)?;
                Ok(())
            }
        }
    }
    fn range_expression(
        &mut self,
        expression: &mut Expr,
        env: &mut Env,
        depth: usize,
    ) -> R<Option<TypeRef>> {
        self.tick(expression.span, depth)?;
        match &mut expression.kind {
            ExprKind::Group(inner) => self.range_expression(inner, env, depth + 1),
            ExprKind::Binary {
                op: BinaryOp::Range,
                left,
                right,
            } => {
                let hint = match self.range_hint(left, env, depth + 1)? {
                    Some(ty) => Some(ty),
                    None => self.range_hint(right, env, depth + 1)?,
                };
                // The lookahead never transforms or evaluates an endpoint.
                // Actual expansion and runtime evaluation keep source order.
                let start = self.expr_expected(left, env, hint.as_ref(), depth + 1)?;
                self.expr_expected(right, env, start.as_ref(), depth + 1)?;
                Ok(start)
            }
            _ => self.expr(expression, env, depth + 1),
        }
    }

    /// Read-only type hints for a range. This does not instantiate a function,
    /// consume an argument, update the AST or confer any typing authority.
    fn range_hint(&mut self, e: &Expr, env: &Env, depth: usize) -> R<Option<TypeRef>> {
        self.tick(e.span, depth)?;
        match &e.kind {
            ExprKind::Name(name) => {
                self.budget.text(name, e.span)?;
                let ty = env.get(name).and_then(Option::as_ref);
                if let Some(ty) = ty {
                    self.budget.ty(ty)?;
                }
                Ok(ty.cloned())
            }
            ExprKind::Literal(Literal::Integer(text)) => {
                self.budget.text(text, e.span)?;
                Ok(["u8", "u16", "i32", "i64"]
                    .into_iter()
                    .find(|suffix| text.ends_with(suffix))
                    .map(|suffix| named(suffix, e.span)))
            }
            ExprKind::Literal(Literal::Bool(_)) => Ok(Some(named("bool", e.span))),
            ExprKind::Group(inner) | ExprKind::MetaEval(inner) => {
                self.range_hint(inner, env, depth + 1)
            }
            ExprKind::Unary {
                op: UnaryOp::Not, ..
            } => Ok(Some(named("bool", e.span))),
            ExprKind::Unary {
                op: UnaryOp::Dereference,
                operand,
            } => {
                let Some(ty) = self.range_hint(operand, env, depth + 1)? else {
                    return Ok(None);
                };
                let TypeKind::Reference { inner, .. } = &grouped(&ty).kind else {
                    return Ok(None);
                };
                self.budget.ty(inner)?;
                Ok(Some(*inner.clone()))
            }
            ExprKind::Unary { operand, .. } => self.range_hint(operand, env, depth + 1),
            ExprKind::Binary {
                op:
                    BinaryOp::Equal
                    | BinaryOp::NotEqual
                    | BinaryOp::Less
                    | BinaryOp::LessEqual
                    | BinaryOp::Greater
                    | BinaryOp::GreaterEqual
                    | BinaryOp::And
                    | BinaryOp::Or,
                ..
            } => Ok(Some(named("bool", e.span))),
            ExprKind::Binary { left, right, .. } => match self.range_hint(left, env, depth + 1)? {
                Some(ty) => Ok(Some(ty)),
                None => self.range_hint(right, env, depth + 1),
            },
            ExprKind::If {
                then_value,
                else_value,
                ..
            } => match self.range_hint(then_value, env, depth + 1)? {
                Some(ty) => Ok(Some(ty)),
                None => self.range_hint(else_value, env, depth + 1),
            },
            ExprKind::Try(operand) => {
                let Some(ty) = self.range_hint(operand, env, depth + 1)? else {
                    return Ok(None);
                };
                let TypeKind::Error { result, .. } = &grouped(&ty).kind else {
                    return Ok(None);
                };
                self.budget.ty(result)?;
                Ok(Some(*result.clone()))
            }
            ExprKind::Index { object, .. } | ExprKind::Member { object, .. } => {
                let Some(ty) = self.range_hint(object, env, depth + 1)? else {
                    return Ok(None);
                };
                let original = self.original_type(projection_shape(&ty))?;
                match (&e.kind, &grouped(&original).kind) {
                    (ExprKind::Index { .. }, TypeKind::Array { element, .. }) => {
                        self.budget.ty(element)?;
                        Ok(Some(*element.clone()))
                    }
                    (
                        ExprKind::Member {
                            member: MemberName::TupleIndex(index),
                            ..
                        },
                        TypeKind::Tuple(fields),
                    ) => {
                        let field = fields.get(*index as usize);
                        if let Some(field) = field {
                            self.budget.ty(field)?;
                        }
                        Ok(field.cloned())
                    }
                    (
                        ExprKind::Member {
                            member: MemberName::Name(member),
                            ..
                        },
                        TypeKind::Named { path, args },
                    ) => {
                        for part in path {
                            self.budget.text(part, e.span)?;
                        }
                        let Some(Item {
                            kind: ItemKind::Aggregate(decl),
                            ..
                        }) = self.nominals.get(&path.join("."))
                        else {
                            return Ok(None);
                        };
                        for field in &decl.fields {
                            self.budget.text(&field.name, e.span)?;
                        }
                        let Some(field) = decl.fields.iter().find(|f| f.name == *member) else {
                            return Ok(None);
                        };
                        if decl.generics.len() != args.len() {
                            return Ok(None);
                        }
                        let mut found = BTreeMap::new();
                        for (parameter, argument) in decl.generics.iter().zip(args) {
                            let GenericArg::Type(ty) = argument else {
                                return Ok(None);
                            };
                            self.budget.text(&parameter.name, e.span)?;
                            self.budget.ty(ty)?;
                            found.insert(parameter.name.clone(), ty.clone());
                        }
                        enums::substituted(&field.ty, &found, &mut self.budget).map(Some)
                    }
                    _ => Ok(None),
                }
            }
            ExprKind::Call { callee, arguments } => {
                if let Some(name) = super::integer_conversion_name(callee) {
                    return Ok(Some(named(name, e.span)));
                }
                if let ExprKind::Member {
                    object,
                    member: MemberName::Name(name),
                } = &callee.kind
                {
                    if name == "len"
                        && self.range_hint(object, env, depth + 1)?.is_some_and(|ty| {
                            matches!(&grouped(&ty).kind, TypeKind::Reference { inner, .. }
                            if matches!(grouped(inner).kind, TypeKind::Array { length: None, .. }))
                        })
                    {
                        return Ok(Some(named("i64", e.span)));
                    }
                    return Ok(None);
                }
                let (target, explicit) = match &callee.kind {
                    ExprKind::Name(name) => (name, None),
                    ExprKind::TypeApply { callee, arguments } => match &callee.kind {
                        ExprKind::Name(name) => (name, Some(arguments)),
                        _ => return Ok(None),
                    },
                    _ => return Ok(None),
                };
                self.budget.text(target, e.span)?;
                if matches!(
                    target.as_str(),
                    "bytes_len" | "bytes_capacity" | "store_len" | "store_capacity"
                ) {
                    return Ok(Some(named("i64", e.span)));
                }
                if let Some(local) = env.get(target) {
                    let result = local.as_ref().and_then(|ty| match &grouped(ty).kind {
                        TypeKind::Function { result, .. } => Some(result.as_ref()),
                        _ => callable::opaque_signature(ty).map(|(_, _, result)| result),
                    });
                    if let Some(result) = result {
                        self.budget.ty(result)?;
                    }
                    return Ok(result.cloned());
                }
                let mut actuals = Vec::new();
                for arg in arguments {
                    actuals.push(self.range_hint(&arg.value, env, depth + 1)?);
                }
                let Some(function) = self.signatures.get(target) else {
                    return Ok(None);
                };
                let Some(result) = &function.return_type else {
                    return Ok(Some(check::unit(e.span)));
                };
                let mut variables = BTreeSet::new();
                for parameter in &function.generics {
                    self.budget.text(&parameter.name, e.span)?;
                    variables.insert(parameter.name.clone());
                }
                self.budget.ty(result)?;
                let mut found = BTreeMap::new();
                if let Some(arguments) = explicit {
                    if arguments.len() != function.generics.len() {
                        return Ok(None);
                    }
                    for (parameter, argument) in function.generics.iter().zip(arguments) {
                        let GenericArg::Type(ty) = argument else {
                            return Ok(None);
                        };
                        self.budget.text(&parameter.name, e.span)?;
                        self.budget.ty(ty)?;
                        found.insert(parameter.name.clone(), ty.clone());
                    }
                }
                for (parameter, actual) in function.params.iter().zip(actuals) {
                    if let Some(actual) = actual {
                        unify(
                            &parameter.ty,
                            &actual,
                            &variables,
                            &mut found,
                            e.span,
                            &mut self.budget,
                        )?;
                    }
                }
                let result = enums::substituted(result, &found, &mut self.budget)?;
                Ok((!check::dependent(&result, &variables)).then_some(result))
            }
            _ => Ok(None),
        }
    }

    fn expr(&mut self, e: &mut Expr, env: &mut Env, depth: usize) -> R<Option<TypeRef>> {
        self.expr_expected(e, env, None, depth)
    }
    fn expr_expected(
        &mut self,
        e: &mut Expr,
        env: &mut Env,
        expected: Option<&TypeRef>,
        depth: usize,
    ) -> R<Option<TypeRef>> {
        self.tick(e.span, depth)?;
        // A surrounding E!T admits either exact payload or the whole union.
        // It therefore cannot constrain a generic result variable to E!T
        // before argument inference determines the expression's own type.
        let store_success_context = expected.filter(|ty| stores::success(ty).is_some());
        let expected = expected.filter(|ty| !matches!(grouped(ty).kind, TypeKind::Error { .. }));
        if let Some(ty) = self.enum_expression(e, env, expected, depth)? {
            return Ok(Some(ty));
        }
        let result = match &mut e.kind {
            ExprKind::Literal(Literal::Integer(text)) => {
                if let Some(ty) = expected.filter(|ty| check::integer(ty)) {
                    self.budget.ty(ty)?;
                    Some(ty.clone())
                } else {
                    Some(named(
                        if text.ends_with("u8") {
                            "u8"
                        } else if text.ends_with("u16") {
                            "u16"
                        } else if text.ends_with("i64") {
                            "i64"
                        } else {
                            "i32"
                        },
                        e.span,
                    ))
                }
            }
            ExprKind::Literal(Literal::Bool(_)) => Some(named("bool", e.span)),
            ExprKind::Name(name) => {
                if let Some(Some(ty)) = env.get(name) {
                    self.budget.ty(ty)?;
                    Some(ty.clone())
                } else {
                    None
                }
            }
            ExprKind::Group(inner) | ExprKind::MetaEval(inner) => {
                self.expr_expected(inner, env, expected.or(store_success_context), depth + 1)?
            }
            ExprKind::Unary { op, operand } => {
                let hint = if matches!(op, UnaryOp::Negate | UnaryOp::BitNot) {
                    expected
                } else {
                    None
                };
                let ty = self.expr_expected(operand, env, hint, depth + 1)?;
                match (*op, ty) {
                    (UnaryOp::Borrow | UnaryOp::BorrowMut, Some(inner)) => Some(TypeRef {
                        span: e.span,
                        kind: TypeKind::Reference {
                            mutable: *op == UnaryOp::BorrowMut,
                            lifetime: None,
                            inner: Box::new(inner),
                        },
                    }),
                    (
                        UnaryOp::Dereference,
                        Some(TypeRef {
                            kind: TypeKind::Reference { inner, .. },
                            ..
                        }),
                    ) => Some(*inner),
                    (_, ty) => ty,
                }
            }
            ExprKind::Binary { op, left, right } => {
                let hint = expected.filter(|_| {
                    !matches!(
                        op,
                        BinaryOp::Equal
                            | BinaryOp::NotEqual
                            | BinaryOp::Less
                            | BinaryOp::LessEqual
                            | BinaryOp::Greater
                            | BinaryOp::GreaterEqual
                    )
                });
                let left_type = self.expr_expected(left, env, hint, depth + 1)?;
                self.expr_expected(right, env, hint.or(left_type.as_ref()), depth + 1)?;
                if matches!(
                    op,
                    BinaryOp::Equal
                        | BinaryOp::NotEqual
                        | BinaryOp::Less
                        | BinaryOp::LessEqual
                        | BinaryOp::Greater
                        | BinaryOp::GreaterEqual
                        | BinaryOp::And
                        | BinaryOp::Or
                ) {
                    Some(named("bool", e.span))
                } else {
                    left_type
                }
            }
            ExprKind::Call { callee, arguments } => {
                if let Some(name) =
                    super::foreign::bare_name(callee).filter(|name| stores::primitive(name))
                {
                    if arguments.len() != stores::arity(name)
                        || arguments.iter().any(|arg| arg.spread)
                    {
                        return Err(error(
                            e.span,
                            "Store primitive argument count or shape differs",
                        ));
                    }
                    let element = if name == "store_new" {
                        expected
                            .or(store_success_context.and_then(stores::carrier))
                            .and_then(stores::element)
                            .cloned()
                            .ok_or_else(|| {
                                error(
                                    e.span,
                                    "store_new requires an explicit Store element context",
                                )
                            })?
                    } else {
                        let store = self
                            .expr_expected(&mut arguments[0].value, env, None, depth + 1)?
                            .ok_or_else(|| {
                                error(e.span, "Store primitive requires a known Store type")
                            })?;
                        stores::element(&store).cloned().ok_or_else(|| {
                            error(e.span, "Store primitive requires a Store description")
                        })?
                    };
                    self.budget.ty(&element)?;
                    if arguments.len() == 2 {
                        let wanted = if name == "store_push" {
                            element.clone()
                        } else {
                            named("i64", e.span)
                        };
                        self.expr_expected(&mut arguments[1].value, env, Some(&wanted), depth + 1)?;
                    }
                    return Ok(Some(stores::result(name, &element, e.span)));
                }
                if let Some((parameters, result)) = super::foreign::bare_name(callee)
                    .and_then(|name| bytes::signature(name, e.span))
                {
                    if arguments.len() != parameters.len() || arguments.iter().any(|arg| arg.spread)
                    {
                        return Err(error(
                            e.span,
                            "byte primitive argument count or shape differs",
                        ));
                    }
                    for (argument, parameter) in arguments.iter_mut().zip(&parameters) {
                        self.expr_expected(&mut argument.value, env, Some(parameter), depth + 1)?;
                    }
                    self.budget.ty(&result)?;
                    return Ok(Some(result));
                }
                if let Some(value) = super::foreign::literal_constructor(callee, arguments, e.span)
                {
                    let value = value?;
                    if let zil_ir::Constant::WideString(units) = &value {
                        self.budget.wide_units(units, e.span)?;
                    }
                    return Ok(Some(named(&value.ty().name(), e.span)));
                }
                if super::foreign::bare_name(callee) == Some("c_callback") {
                    let [argument] = arguments.as_slice() else {
                        return Err(super::foreign::callback_error(
                            e.span,
                            "c_callback requires one static symbol",
                        ));
                    };
                    if argument.spread {
                        return Err(super::foreign::callback_error(
                            e.span,
                            "C callback constructor does not accept spread arguments",
                        ));
                    }
                    let Some(target) = super::foreign::bare_name(&argument.value)
                        .filter(|name| !env.contains_key(*name))
                    else {
                        return Err(super::foreign::callback_error(
                            e.span,
                            "c_callback requires one static symbol",
                        ));
                    };
                    let Some(function) = self.signatures.get(target) else {
                        return Err(super::foreign::callback_error(
                            e.span,
                            "unknown callback symbol",
                        ));
                    };
                    for parameter in &function.params {
                        self.budget.ty(&parameter.ty)?;
                    }
                    if let Some(result) = &function.return_type {
                        self.budget.ty(result)?;
                    }
                    return super::foreign::callback_syntax_type(function, e.span).map(Some);
                }
                if let Some(name) = super::integer_conversion_name(callee) {
                    if arguments.len() != 1 || arguments[0].spread {
                        return Err(super::conversion_error(
                            e.span,
                            "integer conversion requires exactly one non-spread argument",
                        ));
                    }
                    self.expr(&mut arguments[0].value, env, depth + 1)?;
                    return Ok(Some(named(name, e.span)));
                }
                let expected = expected.or_else(|| {
                    let result = stores::call_name(callee)
                        .and_then(|name| self.signatures.get(name))?
                        .return_type
                        .as_ref()?;
                    stores::call_context(result, store_success_context?)
                });
                let value_inference_allowed = matches!(&callee.kind, ExprKind::Name(name)
                    if !env.contains_key(name) && self.signatures.get(name)
                        .is_some_and(|f| store_templates::parameter(f).is_some()));
                let mut value_inferred = false;
                let mut hints = self.call_hints(callee, env, expected)?;
                let mut actual = Vec::new();
                for (index, arg) in arguments.iter_mut().enumerate() {
                    let hint = hints
                        .as_ref()
                        .map(|h| h.hint(index, &mut self.budget))
                        .transpose()?
                        .flatten();
                    let ty = self.expr_expected(&mut arg.value, env, hint.as_ref(), depth + 1)?;
                    if value_inference_allowed
                        && let Some(ty) = &ty
                        && self.inferred_state_value(ty, 0)?
                    {
                        value_inferred = true;
                        // Concrete C identities belong to Types. A prior
                        // symbolic spelling is not a cross-argument equality
                        // constraint; the new static Call checks all real T's.
                        hints = None;
                    }
                    if let (Some(hints), Some(ty)) = (&mut hints, &ty) {
                        let ty = self.original_type(ty)?;
                        hints.constrain(index, &ty, arg.span, &mut self.budget)?;
                    }
                    actual.push(ty);
                }
                let mut explicit = None;
                let target = match &mut callee.kind {
                    ExprKind::TypeApply {
                        callee: base,
                        arguments: args,
                    } => {
                        let mut types = Vec::new();
                        for arg in args {
                            let GenericArg::Type(t) = arg else {
                                return Err(unsupported(e.span, "const generic argument"));
                            };
                            self.ty(t, depth + 1)?;
                            types.push(t.clone());
                        }
                        explicit = Some(types);
                        match &base.kind {
                            ExprKind::Name(n) if !env.contains_key(n) => Some(n.clone()),
                            _ => None,
                        }
                    }
                    ExprKind::Name(n) if !env.contains_key(n) => Some(n.clone()),
                    _ => None,
                };
                if let Some(target) = target {
                    if let Some(template) = self.templates.get(&target).cloned() {
                        let ItemKind::Function(function) = &template.kind else {
                            return Err(error(e.span, "aggregate template is not a callable"));
                        };
                        if let Some(callable) = callable::template(function)? {
                            if explicit.is_some() {
                                return Err(unsupported(
                                    e.span,
                                    "concrete environment arguments are inferred from the supplied closure",
                                ));
                            }
                            if function.params.len() != actual.len() {
                                return Err(error(e.span, "callable helper arity mismatch"));
                            }
                            if function
                                .generics
                                .iter()
                                .any(|p| !callable.contains_generic(&p.name))
                            {
                                let variables: BTreeSet<_> =
                                    function.generics.iter().map(|p| p.name.clone()).collect();
                                let mut found = BTreeMap::new();
                                if let (Some(result), Some(expected)) =
                                    (&function.return_type, expected)
                                {
                                    let expected = self.original_type(expected)?;
                                    unify(
                                        result,
                                        &expected,
                                        &variables,
                                        &mut found,
                                        e.span,
                                        &mut self.budget,
                                    )?;
                                }
                                for (index, (parameter, actual)) in
                                    function.params.iter().zip(actual).enumerate()
                                {
                                    if callable.roles.iter().any(|role| {
                                        role.index == index
                                            && !callable::state_type(
                                                &parameter.ty,
                                                &role.generic_name,
                                            )
                                    }) {
                                        continue;
                                    }
                                    let actual = actual.ok_or_else(|| {
                                        error(
                                            e.span,
                                            "ordinary Copy type argument cannot be inferred",
                                        )
                                    })?;
                                    let actual = self.original_type(&actual)?;
                                    unify_argument(
                                        &parameter.ty,
                                        &actual,
                                        &variables,
                                        &mut found,
                                        e.span,
                                        &mut self.budget,
                                    )?;
                                }
                                let types = function.generics.iter()
                                    .filter(|p| !callable.contains_generic(&p.name))
                                    .map(|p| found.get(&p.name).cloned().ok_or_else(|| error(e.span,
                                        format!("Copy type parameter {} has no unique constraint", p.name))))
                                    .collect::<R<Vec<_>>>()?;
                                let instance = self.instantiate(&target, types, e.span)?;
                                callee.kind = ExprKind::Name(instance.clone());
                                if callable.owned_state
                                    && let (Some(hints), Some(result)) =
                                        (&hints, &function.return_type)
                                    && let Some(mut result) = hints.symbolic_result(
                                        result,
                                        &callable,
                                        &mut self.budget,
                                    )?
                                {
                                    self.ty(&mut result, depth + 1)?;
                                    self.note_single_state_value_result(&result, &callable)?;
                                    return Ok(Some(result));
                                }
                                return self.signature_result(&instance);
                            }
                            if callable.owned_state
                                && let (Some(hints), Some(result)) = (&hints, &function.return_type)
                                && let Some(mut result) =
                                    hints.symbolic_result(result, &callable, &mut self.budget)?
                            {
                                self.ty(&mut result, depth + 1)?;
                                self.note_single_state_value_result(&result, &callable)?;
                                return Ok(Some(result));
                            }
                            return self.signature_result(&target);
                        }
                        if value_inferred {
                            let item = self.templates.get(&target).copied().ok_or_else(|| {
                                error(e.span, "inferred ordinary call has no original template")
                            })?;
                            return self.apply_value_store_template(
                                callee, function, item, &target, &actual, e.span,
                            );
                        }
                        let types = if let Some(types) = explicit {
                            types
                        } else {
                            if function.params.len() != actual.len() {
                                return Err(error(e.span, "generic call arity mismatch"));
                            }
                            let variables =
                                function.generics.iter().map(|p| p.name.clone()).collect();
                            let mut found = BTreeMap::new();
                            if let (Some(result), Some(expected)) =
                                (&function.return_type, expected)
                            {
                                let expected = self.original_type(expected)?;
                                unify(
                                    result,
                                    &expected,
                                    &variables,
                                    &mut found,
                                    e.span,
                                    &mut self.budget,
                                )?;
                            }
                            for (p, actual) in function.params.iter().zip(actual) {
                                let actual = actual.ok_or_else(|| {
                                    error(
                                        e.span,
                                        "generic argument cannot be inferred; use explicit ::<...>",
                                    )
                                })?;
                                let actual = self.original_type(&actual)?;
                                unify_argument(
                                    &p.ty,
                                    &actual,
                                    &variables,
                                    &mut found,
                                    e.span,
                                    &mut self.budget,
                                )?;
                            }
                            function
                                .generics
                                .iter()
                                .map(|p| {
                                    found.get(&p.name).cloned().ok_or_else(|| {
                                        error(
                                            e.span,
                                            format!(
                                                "type parameter {} has no unique constraint",
                                                p.name
                                            ),
                                        )
                                    })
                                })
                                .collect::<R<Vec<_>>>()?
                        };
                        match self.instantiate_inner(&target, types, e.span, true)? {
                            store_templates::Instantiation::Eager(instance) => {
                                callee.kind = ExprKind::Name(instance.clone());
                                self.signature_result(&instance)?
                            }
                            store_templates::Instantiation::Deferred { template, element } => {
                                self.apply_store_template(callee, function, template, element)?
                            }
                        }
                    } else {
                        if explicit.is_some() {
                            return Err(error(
                                e.span,
                                "type arguments supplied to a non-generic function",
                            ));
                        }
                        self.signature_result(&target)?
                    }
                } else {
                    self.expr(callee, env, depth + 1)?;
                    None
                }
            }
            ExprKind::Tuple(values) => {
                let mut types = Vec::new();
                let mut known = true;
                let hints = expected.and_then(|t| match &grouped(t).kind {
                    TypeKind::Tuple(ts) => Some(ts.as_slice()),
                    _ => None,
                });
                for (index, value) in values.iter_mut().enumerate() {
                    if let Some(ty) = self.expr_expected(
                        value,
                        env,
                        hints.and_then(|ts| ts.get(index)),
                        depth + 1,
                    )? {
                        types.push(ty);
                    } else {
                        known = false;
                    }
                }
                known.then_some(TypeRef {
                    kind: TypeKind::Tuple(types),
                    span: e.span,
                })
            }
            ExprKind::Array(values) => {
                let mut element = expected
                    .and_then(|ty| match &grouped(ty).kind {
                        TypeKind::Array { element, .. } => Some(element.as_ref()),
                        _ => None,
                    })
                    .cloned();
                for value in values.iter_mut() {
                    let actual = self.expr_expected(value, env, element.as_ref(), depth + 1)?;
                    if element.is_none() {
                        element = actual;
                    }
                }
                element
                    .map(|element| array_type(element, values.len(), e.span))
                    .transpose()?
            }
            ExprKind::Member { object, member } => {
                let object_type = self.expr(object, env, depth + 1)?;
                match (object_type.as_ref().map(projection_shape), member) {
                    (
                        Some(TypeRef {
                            kind: TypeKind::Tuple(fields),
                            ..
                        }),
                        MemberName::TupleIndex(i),
                    ) => fields.get(*i as usize).cloned(),
                    (Some(ty), MemberName::Name(member)) => self.record_field(ty, member)?,
                    _ => None,
                }
            }
            ExprKind::Index { object, index } => {
                let object_type = self.expr(object, env, depth + 1)?;
                self.expr(index, env, depth + 1)?;
                match object_type.as_ref().map(projection_shape) {
                    Some(TypeRef {
                        kind: TypeKind::Array { element, .. },
                        ..
                    }) => {
                        self.budget.ty(element)?;
                        Some(*element.clone())
                    }
                    _ => None,
                }
            }
            ExprKind::Slice { object, start, end } => {
                let object_type = self.expr(object, env, depth + 1)?;
                for value in [start, end].into_iter().flatten() {
                    self.expr(value, env, depth + 1)?;
                }
                match object_type.as_ref().map(projection_shape) {
                    Some(TypeRef {
                        kind: TypeKind::Array { element, .. },
                        ..
                    }) => {
                        self.budget.ty(element)?;
                        Some(TypeRef {
                            span: e.span,
                            kind: TypeKind::Array {
                                element: element.clone(),
                                length: None,
                            },
                        })
                    }
                    _ => None,
                }
            }
            ExprKind::If {
                condition,
                then_value,
                else_value,
            } => {
                self.expr(condition, env, depth + 1)?;
                let ty = self.expr_expected(then_value, env, expected, depth + 1)?;
                self.expr_expected(else_value, env, expected.or(ty.as_ref()), depth + 1)?;
                ty
            }
            ExprKind::Record { ty, base, fields } => {
                if let Some(t) = ty
                    && let ExprKind::TypeApply { callee, arguments } = &mut t.kind
                    && let ExprKind::Name(template) = &callee.kind
                    && matches!(
                        self.templates.get(template).map(|x| &x.kind),
                        Some(ItemKind::Aggregate(_))
                    )
                {
                    let mut types = Vec::new();
                    for arg in arguments {
                        let GenericArg::Type(value) = arg else {
                            return Err(unsupported(e.span, "const generic argument"));
                        };
                        self.ty(value, depth + 1)?;
                        types.push(value.clone());
                    }
                    t.kind = ExprKind::Name(self.instantiate(template, types, e.span)?);
                }
                let explicit_type = ty.as_ref().and_then(|t| match &t.kind {
                    ExprKind::Name(n) => Some(named(n, t.span)),
                    _ => None,
                });
                let base_type = if let Some(base) = base {
                    self.expr_expected(base, env, explicit_type.as_ref(), depth + 1)?
                } else {
                    None
                };
                let record_type = explicit_type.or(base_type);
                for field in fields {
                    if let Some(value) = &mut field.value {
                        let hint = record_type
                            .as_ref()
                            .map(|ty| self.record_field(ty, &field.name))
                            .transpose()?
                            .flatten();
                        self.expr_expected(value, env, hint.as_ref(), depth + 1)?;
                    }
                }
                record_type
            }

            ExprKind::Closure {
                params,
                return_type,
                body,
                ..
            } => {
                let mut inner = self.clone_env(env)?;
                for param in params {
                    if let Some(t) = &mut param.ty {
                        self.ty(t, depth + 1)?;
                    }
                    inner.insert(param.name.clone(), param.ty.clone());
                }
                if let Some(t) = return_type {
                    self.ty(t, depth + 1)?;
                }
                let saved = std::mem::replace(&mut self.return_type, return_type.clone());
                match body {
                    ArrowBody::Block(b) => self.block(b, &mut inner, depth + 1)?,
                    ArrowBody::Expression(e) => {
                        let expected = self.return_type.clone();
                        self.expr_expected(e, &mut inner, expected.as_ref(), depth + 1)?;
                    }
                }
                self.return_type = saved;
                None
            }
            ExprKind::MetaBlock(block) => {
                self.block(block, env, depth + 1)?;
                None
            }
            ExprKind::Try(inner) => match self.expr(inner, env, depth + 1)? {
                Some(TypeRef {
                    kind: TypeKind::Error { result, .. },
                    ..
                }) => Some(*result),
                _ => None,
            },
            ExprKind::Pipeline { input, target } => {
                self.expr(input, env, depth + 1)?;
                self.expr(target, env, depth + 1)?;
                None
            }
            _ => None,
        };
        Ok(result)
    }
    fn signature_result(&mut self, name: &str) -> R<Option<TypeRef>> {
        let callable = self
            .signatures
            .get(name)
            .map(|f| callable::template(f))
            .transpose()?
            .flatten();
        let callable_name = callable
            .as_ref()
            .and_then(|bound| bound.single_generic().map(str::to_owned));
        if let Some(ty) = self
            .signatures
            .get(name)
            .and_then(|f| f.return_type.as_ref())
        {
            self.budget.ty(ty)?;
            let mut result = ty.clone();
            let pair = callable::dual_scope(callable.as_ref(), &mut self.budget, ty.span)?;
            let saved_pair = std::mem::replace(&mut self.callable_pair, pair);
            let saved_callable = std::mem::replace(&mut self.callable_name, callable_name);
            self.ty(&mut result, 0)?;
            self.callable_name = saved_callable;
            self.callable_pair = saved_pair;
            if let Some(names) = callable
                .as_ref()
                .and_then(callable::CallableTemplates::dual_names)
            {
                self.note_state_value_result(&result, callable::StoreCallbacks::Pair(names), 0)?;
            } else if let Some(callable) = &callable {
                self.note_single_state_value_result(&result, callable)?;
            }
            Ok(Some(result))
        } else {
            Ok(None)
        }
    }
    fn clone_env(&mut self, env: &Env) -> R<Env> {
        for ty in env.values().flatten() {
            self.budget.ty(ty)?;
        }
        Ok(env.clone())
    }
    fn original_type(&mut self, ty: &TypeRef) -> R<TypeRef> {
        original_type(ty, &self.instance_types, &mut self.budget, 0)
    }
}

fn original_type(
    ty: &TypeRef,
    instances: &BTreeMap<String, TypeRef>,
    budget: &mut Budget<'_>,
    depth: usize,
) -> R<TypeRef> {
    original_type_observed(
        ty,
        instances,
        budget,
        depth,
        callable::StoreCallbacks::None,
        &mut false,
    )
}

fn original_type_observed(
    ty: &TypeRef,
    instances: &BTreeMap<String, TypeRef>,
    budget: &mut Budget<'_>,
    depth: usize,
    binder: callable::StoreCallbacks<'_>,
    dependent: &mut bool,
) -> R<TypeRef> {
    budget.node(ty.span, depth)?;
    budget.ty(ty)?;
    let kind = match &ty.kind {
        TypeKind::Named { path, args } => {
            // Observe the already charged node, including names restored from
            // a nominal instance. No second traversal changes legacy budgets.
            if args.is_empty() && path.len() == 1 && binder.contains(&path[0]) {
                *dependent = true;
            }
            if path.len() == 1
                && args.is_empty()
                && let Some(original) = instances.get(&path[0])
            {
                return original_type_observed(
                    original,
                    instances,
                    budget,
                    depth + 1,
                    binder,
                    dependent,
                );
            }
            TypeKind::Named {
                path: path.clone(),
                args: args
                    .iter()
                    .map(|arg| match arg {
                        GenericArg::Type(t) => Ok(GenericArg::Type(original_type_observed(
                            t,
                            instances,
                            budget,
                            depth + 1,
                            binder,
                            dependent,
                        )?)),
                        _ => Err(unsupported(ty.span, "const generic argument")),
                    })
                    .collect::<R<_>>()?,
            }
        }
        TypeKind::Reference {
            lifetime,
            mutable,
            inner,
        } => TypeKind::Reference {
            lifetime: lifetime.clone(),
            mutable: *mutable,
            inner: Box::new(original_type_observed(
                inner,
                instances,
                budget,
                depth + 1,
                binder,
                dependent,
            )?),
        },
        TypeKind::Group(inner) => TypeKind::Group(Box::new(original_type_observed(
            inner,
            instances,
            budget,
            depth + 1,
            binder,
            dependent,
        )?)),
        TypeKind::Optional(inner) => TypeKind::Optional(Box::new(original_type_observed(
            inner,
            instances,
            budget,
            depth + 1,
            binder,
            dependent,
        )?)),
        TypeKind::Tuple(ts) => TypeKind::Tuple(
            ts.iter()
                .map(|t| original_type_observed(t, instances, budget, depth + 1, binder, dependent))
                .collect::<R<_>>()?,
        ),
        TypeKind::Array { element, length } => TypeKind::Array {
            element: Box::new(original_type_observed(
                element,
                instances,
                budget,
                depth + 1,
                binder,
                dependent,
            )?),
            length: length.clone(),
        },
        TypeKind::Error { error, result } => TypeKind::Error {
            error: Box::new(original_type_observed(
                error,
                instances,
                budget,
                depth + 1,
                binder,
                dependent,
            )?),
            result: Box::new(original_type_observed(
                result,
                instances,
                budget,
                depth + 1,
                binder,
                dependent,
            )?),
        },
        TypeKind::ForeignFunction {
            abi,
            params,
            result,
        } => TypeKind::ForeignFunction {
            abi: abi.clone(),
            params: params
                .iter()
                .map(|t| original_type_observed(t, instances, budget, depth + 1, binder, dependent))
                .collect::<R<_>>()?,
            result: Box::new(original_type_observed(
                result,
                instances,
                budget,
                depth + 1,
                binder,
                dependent,
            )?),
        },
        TypeKind::Function { params, result } => TypeKind::Function {
            params: params
                .iter()
                .map(|t| original_type_observed(t, instances, budget, depth + 1, binder, dependent))
                .collect::<R<_>>()?,
            result: Box::new(original_type_observed(
                result,
                instances,
                budget,
                depth + 1,
                binder,
                dependent,
            )?),
        },
        _ => ty.kind.clone(),
    };
    Ok(TypeRef {
        span: ty.span,
        kind,
    })
}
fn array_length(length: &Expr) -> R<u32> {
    match &length.kind {
        ExprKind::Literal(Literal::Integer(text)) => {
            text.replace('_', "").parse::<u32>().map_err(|_| {
                unsupported(
                    length.span,
                    "array length must be a bounded integer constant",
                )
            })
        }
        _ => Err(unsupported(
            length.span,
            "nonliteral array length needs CT type evaluation",
        )),
    }
}
fn array_type(element: TypeRef, length: usize, span: Span) -> R<TypeRef> {
    let length = u32::try_from(length)
        .map_err(|_| error(span, "array literal exceeds static length representation"))?;
    Ok(TypeRef {
        span,
        kind: TypeKind::Array {
            element: Box::new(element),
            length: Some(Box::new(Expr {
                span,
                kind: ExprKind::Literal(Literal::Integer(length.to_string())),
            })),
        },
    })
}
fn grouped(mut ty: &TypeRef) -> &TypeRef {
    while let TypeKind::Group(t) = &ty.kind {
        ty = t;
    }
    ty
}

/// Match the one implicit address projection already accepted by Types.
/// This is only a syntax type hint; it grants neither access nor ownership.
fn projection_shape(ty: &TypeRef) -> &TypeRef {
    match &grouped(ty).kind {
        TypeKind::Reference { inner, .. } => grouped(inner),
        _ => grouped(ty),
    }
}

fn bind(pattern: &Pattern, ty: Option<TypeRef>, env: &mut Env) {
    match &pattern.kind {
        PatternKind::Name(n) => {
            env.insert(n.clone(), ty);
        }
        PatternKind::Tuple(patterns) => {
            let types = match ty {
                Some(TypeRef {
                    kind: TypeKind::Tuple(types),
                    ..
                }) => types,
                _ => vec![],
            };
            for (i, p) in patterns.iter().enumerate() {
                bind(p, types.get(i).cloned(), env);
            }
        }
        _ => {}
    }
}
fn type_key(ty: &TypeRef, budget: &mut Budget<'_>) -> R<String> {
    budget.ty(ty)?;
    type_key_bounded(ty)
}
fn type_key_bounded(ty: &TypeRef) -> R<String> {
    Ok(match &ty.kind {
        TypeKind::Named { path, args } if path.as_slice() == ["void"] && args.is_empty() => {
            "()".into()
        }
        TypeKind::Named { path, args } => {
            let mut key = format!("N{}:{}", path.join(".").len(), path.join("."));
            for a in args {
                let GenericArg::Type(t) = a else {
                    return Err(unsupported(ty.span, "const generic argument"));
                };
                key.push_str(&format!("<{}>", type_key_bounded(t)?));
            }
            key
        }
        TypeKind::Group(t) => return type_key_bounded(t),
        TypeKind::Reference {
            mutable,
            inner,
            lifetime: None,
        } => format!("R{mutable}<{}>", type_key_bounded(inner)?),
        TypeKind::Tuple(ts) => format!(
            "({})",
            ts.iter()
                .map(type_key_bounded)
                .collect::<R<Vec<_>>>()?
                .join(",")
        ),
        TypeKind::Array {
            element,
            length: Some(length),
        } => format!("A{}<{}>", array_length(length)?, type_key_bounded(element)?),
        TypeKind::Array {
            element,
            length: None,
        } => format!("S<{}>", type_key_bounded(element)?),
        TypeKind::Optional(t) => format!("?{}", type_key_bounded(t)?),
        TypeKind::Error { error, result } => format!(
            "E<{}><{}>",
            type_key_bounded(error)?,
            type_key_bounded(result)?
        ),
        TypeKind::ForeignFunction {
            abi,
            params,
            result,
        } => format!(
            "C{}:{}({})->{}",
            abi.len(),
            abi,
            params
                .iter()
                .map(type_key_bounded)
                .collect::<R<Vec<_>>>()?
                .join(","),
            type_key_bounded(result)?
        ),
        TypeKind::Function { params, result } => format!(
            "F({})->{}",
            params
                .iter()
                .map(type_key_bounded)
                .collect::<R<Vec<_>>>()?
                .join(","),
            type_key_bounded(result)?
        ),
        _ => {
            return Err(unsupported(
                ty.span,
                "this generic type argument needs additional type resolution",
            ));
        }
    })
}
fn unify(
    pattern: &TypeRef,
    actual: &TypeRef,
    variables: &BTreeSet<String>,
    found: &mut BTreeMap<String, TypeRef>,
    span: Span,
    budget: &mut Budget<'_>,
) -> R<()> {
    budget.ty(pattern)?;
    budget.ty(actual)?;
    unify_bounded(pattern, actual, variables, found, span)
}
fn unify_argument(
    pattern: &TypeRef,
    actual: &TypeRef,
    variables: &BTreeSet<String>,
    found: &mut BTreeMap<String, TypeRef>,
    span: Span,
    budget: &mut Budget<'_>,
) -> R<()> {
    budget.ty(pattern)?;
    budget.ty(actual)?;
    let mut pattern = pattern;
    while let TypeKind::Group(inner) = &pattern.kind {
        pattern = inner;
    }
    let mut actual = actual;
    while let TypeKind::Group(inner) = &actual.kind {
        actual = inner;
    }
    if let (
        TypeKind::Reference {
            mutable: wanted,
            inner: expected,
            ..
        },
        TypeKind::Reference {
            mutable: available,
            inner: actual,
            ..
        },
    ) = (&pattern.kind, &actual.kind)
    {
        if *wanted && !available {
            return Err(error(
                span,
                "shared reference cannot grant exclusive access",
            ));
        }
        return unify_bounded(expected, actual, variables, found, span);
    }
    unify_bounded(pattern, actual, variables, found, span)
}
fn unify_bounded(
    pattern: &TypeRef,
    actual: &TypeRef,
    variables: &BTreeSet<String>,
    found: &mut BTreeMap<String, TypeRef>,
    span: Span,
) -> R<()> {
    if let TypeKind::Group(t) = &pattern.kind {
        return unify_bounded(t, actual, variables, found, span);
    }
    if let TypeKind::Group(t) = &actual.kind {
        return unify_bounded(pattern, t, variables, found, span);
    }
    if let TypeKind::Named { path, args } = &pattern.kind
        && path.len() == 1
        && args.is_empty()
        && variables.contains(&path[0])
    {
        if let Some(prior) = found.get(&path[0]) {
            if type_key_bounded(prior)? != type_key_bounded(actual)? {
                return Err(error(span, "conflicting generic argument constraints"));
            }
        } else {
            found.insert(path[0].clone(), actual.clone());
        }
        return Ok(());
    }
    match (&pattern.kind, &actual.kind) {
        (
            TypeKind::Error {
                error: a,
                result: x,
            },
            TypeKind::Error {
                error: b,
                result: y,
            },
        ) => {
            unify_bounded(a, b, variables, found, span)?;
            unify_bounded(x, y, variables, found, span)
        }
        (TypeKind::Named { path: a, args: x }, TypeKind::Named { path: b, args: y })
            if a == b && x.len() == y.len() =>
        {
            for (a, b) in x.iter().zip(y) {
                let (GenericArg::Type(a), GenericArg::Type(b)) = (a, b) else {
                    return Err(unsupported(span, "const generic unification"));
                };
                unify_bounded(a, b, variables, found, span)?;
            }
            Ok(())
        }
        (
            TypeKind::Reference {
                mutable: a,
                inner: x,
                ..
            },
            TypeKind::Reference {
                mutable: b,
                inner: y,
                ..
            },
        ) if a == b => unify_bounded(x, y, variables, found, span),
        (TypeKind::Tuple(a), TypeKind::Tuple(b)) if a.len() == b.len() => {
            for (a, b) in a.iter().zip(b) {
                unify_bounded(a, b, variables, found, span)?;
            }
            Ok(())
        }
        (
            TypeKind::Array {
                element: a,
                length: Some(x),
            },
            TypeKind::Array {
                element: b,
                length: Some(y),
            },
        ) if array_length(x)? == array_length(y)? => unify_bounded(a, b, variables, found, span),
        (
            TypeKind::Array {
                element: a,
                length: None,
            },
            TypeKind::Array {
                element: b,
                length: None,
            },
        ) => unify_bounded(a, b, variables, found, span),
        _ if type_key_bounded(pattern)? == type_key_bounded(actual)? => Ok(()),
        _ => Err(error(
            span,
            "generic parameter does not match argument type",
        )),
    }
}

// The visitor substitutes only type syntax. Value names are never textual
// replacements: a parameter called T does not become the name of its type.
fn substitute_item(item: &mut Item, substitutions: &BTreeMap<String, TypeRef>) -> R<()> {
    let visitor = Substitution { substitutions };
    match &mut item.kind {
        ItemKind::Function(f) => {
            for parameter in &mut f.generics {
                if let GenericParamKind::Type(bounds) = &mut parameter.kind {
                    for bound in bounds {
                        visitor.ty(bound);
                    }
                }
            }
            for p in &mut f.params {
                visitor.ty(&mut p.ty);
            }
            if let Some(t) = &mut f.return_type {
                visitor.ty(t);
            }
            match &mut f.body {
                FunctionBody::Block(b) => visitor.block(b),
                FunctionBody::Expression(e) => visitor.expr(e),
                _ => {}
            }
        }
        ItemKind::Aggregate(a) => {
            for field in &mut a.fields {
                visitor.ty(&mut field.ty);
                if let Some(e) = &mut field.default {
                    visitor.expr(e);
                }
            }
        }
        ItemKind::Enum(e) => {
            for variant in &mut e.variants {
                match &mut variant.fields {
                    VariantFields::Tuple(fields) => {
                        for field in fields {
                            visitor.ty(field);
                        }
                    }
                    VariantFields::Record(fields) => {
                        for field in fields {
                            visitor.ty(&mut field.ty);
                        }
                    }
                    VariantFields::Unit => {}
                }
            }
        }
        _ => {}
    }
    Ok(())
}
struct Substitution<'a> {
    substitutions: &'a BTreeMap<String, TypeRef>,
}
impl Substitution<'_> {
    fn ty(&self, ty: &mut TypeRef) {
        if let TypeKind::Named { path, args } = &ty.kind
            && path.len() == 1
            && args.is_empty()
            && let Some(value) = self.substitutions.get(&path[0])
        {
            let span = ty.span;
            *ty = value.clone();
            ty.span = span;
            return;
        }
        match &mut ty.kind {
            TypeKind::Named { args, .. } | TypeKind::Dyn { args, .. } => {
                for a in args {
                    if let GenericArg::Type(t) = a {
                        self.ty(t);
                    }
                }
            }
            TypeKind::Reference { inner, .. }
            | TypeKind::Group(inner)
            | TypeKind::Optional(inner)
            | TypeKind::ForLifetime { inner, .. } => self.ty(inner),
            TypeKind::Tuple(ts) | TypeKind::Union(ts) | TypeKind::Impl(ts) => {
                for t in ts {
                    self.ty(t);
                }
            }
            TypeKind::Array { element, length } => {
                self.ty(element);
                if let Some(e) = length {
                    self.expr(e);
                }
            }
            TypeKind::Function { params, result }
            | TypeKind::ForeignFunction { params, result, .. } => {
                for t in params {
                    self.ty(t);
                }
                self.ty(result);
            }
            TypeKind::Error { error, result } => {
                self.ty(error);
                self.ty(result);
            }
            _ => {}
        }
    }
    fn condition(&self, c: &mut Condition) {
        match c {
            Condition::Expression(e) | Condition::Let { value: e, .. } => self.expr(e),
        }
    }
    fn block(&self, b: &mut Block) {
        for s in &mut b.statements {
            self.statement(s);
        }
    }
    fn statement(&self, s: &mut Stmt) {
        match &mut s.kind {
            StmtKind::Binding { ty, value, .. } => {
                if let Some(t) = ty {
                    self.ty(t);
                }
                self.expr(value);
            }
            StmtKind::Return(Some(e)) | StmtKind::Expression(e) | StmtKind::Yield(e) => {
                self.expr(e)
            }
            StmtKind::Assign { target, value, .. } => {
                self.expr(target);
                self.expr(value);
            }
            StmtKind::If {
                condition,
                then_body,
                else_body,
            } => {
                self.condition(condition);
                self.block(then_body);
                match else_body {
                    Some(ElseBranch::Block(b)) => self.block(b),
                    Some(ElseBranch::If(s)) => self.statement(s),
                    None => {}
                }
            }
            StmtKind::While { condition, body } => {
                self.condition(condition);
                self.block(body);
            }
            StmtKind::For { iterable, body, .. } => {
                self.expr(iterable);
                self.block(body);
            }
            StmtKind::Loop(b) | StmtKind::Unsafe(b) | StmtKind::Defer(b) => self.block(b),
            StmtKind::With { value, body, .. } => {
                self.expr(value);
                self.block(body);
            }
            StmtKind::Match { value, arms } => {
                self.expr(value);
                for arm in arms {
                    self.arrow(&mut arm.body);
                }
            }
            _ => {}
        }
    }
    fn arrow(&self, body: &mut ArrowBody) {
        match body {
            ArrowBody::Block(b) => self.block(b),
            ArrowBody::Expression(e) => self.expr(e),
        }
    }
    fn expr(&self, e: &mut Expr) {
        match &mut e.kind {
            ExprKind::Group(x)
            | ExprKind::Try(x)
            | ExprKind::MetaEval(x)
            | ExprKind::Unary { operand: x, .. } => self.expr(x),
            ExprKind::Tuple(xs) | ExprKind::Array(xs) => {
                for x in xs {
                    self.expr(x);
                }
            }
            ExprKind::Binary { left, right, .. } => {
                self.expr(left);
                self.expr(right);
            }
            ExprKind::Pipeline { input, target } => {
                self.expr(input);
                self.expr(target);
            }
            ExprKind::Member { object, .. } => self.expr(object),
            ExprKind::TypeApply { callee, arguments } => {
                self.expr(callee);
                for a in arguments {
                    match a {
                        GenericArg::Type(t) => self.ty(t),
                        GenericArg::Const(e) => self.expr(e),
                    }
                }
            }
            ExprKind::Call { callee, arguments } => {
                self.expr(callee);
                for a in arguments {
                    self.expr(&mut a.value);
                }
            }
            ExprKind::Index { object, index } => {
                self.expr(object);
                self.expr(index);
            }
            ExprKind::Slice { object, start, end } => {
                self.expr(object);
                for x in [start, end].into_iter().flatten() {
                    self.expr(x);
                }
            }
            ExprKind::Record { ty, base, fields } => {
                if let Some(t) = ty {
                    self.expr(t);
                }
                if let Some(b) = base {
                    self.expr(b);
                }
                for f in fields {
                    if let Some(v) = &mut f.value {
                        self.expr(v);
                    }
                }
            }
            ExprKind::Closure {
                params,
                return_type,
                body,
                ..
            } => {
                for p in params {
                    if let Some(t) = &mut p.ty {
                        self.ty(t);
                    }
                }
                if let Some(t) = return_type {
                    self.ty(t);
                }
                self.arrow(body);
            }
            ExprKind::If {
                condition,
                then_value,
                else_value,
            } => {
                self.expr(condition);
                self.expr(then_value);
                self.expr(else_value);
            }
            ExprKind::MetaBlock(b) => self.block(b),
            _ => {}
        }
    }
}

#[path = "generic_check.rs"]
mod check;
use check::validate_parametric;
