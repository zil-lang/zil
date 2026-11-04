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
#[path = "generic_const.rs"]
mod consts;
#[path = "generic_copy.rs"]
mod copy;
#[path = "generic_enum.rs"]
mod enums;
#[path = "generic_helper_graph.rs"]
mod helper_graph;
#[path = "generic_options.rs"]
mod options;
#[path = "generic_products.rs"]
pub(super) mod products;
#[path = "generic_record_enum.rs"]
pub(super) mod record_enums;
#[path = "generic_state.rs"]
mod states;
#[path = "generic_store_templates.rs"]
pub(super) mod store_templates;
#[path = "generic_store.rs"]
mod stores;
#[path = "generic_trait_bounds.rs"]
mod trait_bounds;
#[path = "generic_trait_defaults.rs"]
pub(super) mod trait_defaults;
#[path = "generic_trait_impls.rs"]
mod trait_impls;
#[path = "generic_trait_instances.rs"]
pub(super) mod trait_instances;
#[path = "generic_where.rs"]
mod where_copy;

fn check_nominal_name(name: &str, span: Span) -> R<()> {
    if name == "Option" || super::byte_buffer_name(name) || super::owned_store_name(name) {
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

pub(super) fn specialize_products<'a>(
    module: &'a Module,
    limits: &Limits,
    products: &mut products::Products,
) -> CompileResult<Cow<'a, Module>> {
    let (prepared, remaining) = options::prepare(module, limits)?;
    match prepared {
        Some(prepared) => specialize_products_prepared(&prepared, &remaining, products)
            .map(|module| Cow::Owned(module.into_owned())),
        None => specialize_products_prepared(module, &remaining, products),
    }
}

fn specialize_products_prepared<'a>(
    module: &'a Module,
    limits: &Limits,
    products: &mut products::Products,
) -> CompileResult<Cow<'a, Module>> {
    let mut names_budget = Budget::new(limits);
    names_budget.target_pointer_width = products.target_pointer_width.unwrap_or(64);
    super::lifetimes::validate(module, &mut names_budget)?;
    let mut has_where = false;
    let mut has_defaults = false;
    let mut has_associated = false;
    let mut has_impl_recipes = false;
    let mut has_alias = false;
    for item in &module.items {
        names_budget.node(item.span, 0).map_err(|e| vec![e])?;
        has_where |= matches!(&item.kind, ItemKind::Function(f) if !f.where_clause.is_empty());
        has_alias |= matches!(item.kind, ItemKind::TypeAlias(_));
        has_impl_recipes |= matches!(&item.kind, ItemKind::Impl(i) if i.trait_type.is_some() && (!i.generics.is_empty() || !i.where_clause.is_empty() || trait_bounds::plain(&i.target).is_none()));
        if let ItemKind::Trait(declaration) = &item.kind {
            let (defaults, associated) =
                trait_defaults::features(declaration, &mut names_budget).map_err(|e| vec![e])?;
            has_associated |= associated;
            has_defaults |= defaults || associated;
            has_defaults |= !declaration.bounds.is_empty();
        }
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
            if matches!(param.kind, GenericParamKind::Lifetime) {
                continue;
            }
            names_budget.node(param.span, 0).map_err(|e| vec![e])?;
            if param.name == "Option"
                || super::byte_buffer_name(&param.name)
                || super::owned_store_name(&param.name)
            {
                return Err(vec![super::primitive_name_error(&param.name, param.span)]);
            }
        }
    }
    if names_budget.has_primitive_binders() && !has_alias {
        // The existing scoped normalizer also protects aliases. Without an
        // alias, trigger it only for actual builtin/type-binder collisions.
        // Complete preflight precedes its whole-AST clone, on this same ledger.
        for item in &module.items {
            names_budget.item(item).map_err(|e| vec![e])?;
        }
        let protected = super::modules::protect_alias_binders(module, &mut names_budget)
            .map_err(|e| vec![e])?;
        let protected = if has_where {
            Cow::Owned(
                where_copy::normalize(protected.as_ref(), &mut names_budget)
                    .map_err(|e| vec![e])?,
            )
        } else {
            protected
        };
        return specialize_with_budget(
            protected.as_ref(),
            limits,
            &BTreeMap::new(),
            names_budget,
            products,
            has_associated,
        )
        .map(Cow::Owned)
        .map_err(|e| vec![e]);
    }
    if has_where {
        let normalized = where_copy::normalize(module, &mut names_budget).map_err(|e| vec![e])?;
        let (aliased, budget) =
            aliases::normalize_with_budget(&normalized, names_budget).map_err(|e| vec![e])?;
        return match aliased {
            Some(aliased) => specialize_with_budget(
                &aliased.module,
                limits,
                &aliased.pattern_types,
                budget,
                products,
                has_associated,
            ),
            None => specialize_with_budget(
                &normalized,
                limits,
                &BTreeMap::new(),
                budget,
                products,
                has_associated,
            ),
        }
        .map(Cow::Owned)
        .map_err(|e| vec![e]);
    }
    let (normalized, names_budget) =
        aliases::normalize_with_budget(module, names_budget).map_err(|e| vec![e])?;
    if let Some(normalized) = normalized {
        if normalized.pattern_types.is_empty()
            && normalized
                .module
                .items
                .iter()
                .all(|item| params(item).is_empty())
            && !has_defaults
            && !has_impl_recipes
            && !normalized
                .module
                .items
                .iter()
                .any(|item| matches!(&item.kind,ItemKind::Trait(t) if !t.generics.is_empty()))
            && !names_budget.has_enum_profiles()
        {
            return Ok(Cow::Owned(normalized.module));
        }
        return specialize_with_budget(
            &normalized.module,
            limits,
            &normalized.pattern_types,
            names_budget,
            products,
            has_associated,
        )
        .map(Cow::Owned)
        .map_err(|error| vec![error]);
    }
    if module.items.iter().all(|item| params(item).is_empty())
        && !has_defaults
        && !has_impl_recipes
        && !module
            .items
            .iter()
            .any(|item| matches!(&item.kind,ItemKind::Trait(t) if !t.generics.is_empty()))
        && !names_budget.has_enum_profiles()
    {
        return Ok(Cow::Borrowed(module));
    }
    specialize_with_budget(
        module,
        limits,
        &BTreeMap::new(),
        names_budget,
        products,
        has_associated,
    )
    .map(Cow::Owned)
    .map_err(|error| vec![error])
}

fn specialize_with_budget<'a>(
    module: &'a Module,
    limits: &'a Limits,
    pattern_types: &'a BTreeMap<String, TypeRef>,
    mut budget: Budget<'a>,
    products: &mut products::Products,
    has_associated: bool,
) -> R<Module> {
    let prepared = trait_bounds::associated::prepare(module, has_associated, &mut budget)?;
    let module = prepared.as_ref().unwrap_or(module);
    let mut engine = Engine::with_budget(module, limits, pattern_types, budget)?;
    if !engine.nominal_binder_collision {
        return engine.run(module, products);
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
    engine.run(&protected, products)
}

struct Engine<'a> {
    pattern_types: &'a BTreeMap<String, TypeRef>,
    templates: BTreeMap<String, &'a Item>,
    enums: BTreeMap<String, &'a EnumDecl>,
    nominals: BTreeMap<String, &'a Item>,
    traits: trait_bounds::Index<'a>,
    trait_calls: trait_instances::Calls,
    return_type: Option<TypeRef>,
    callable_name: Option<String>,
    callable_pair: Option<[String; 2]>,
    active_callables: Option<callable::CallableTemplates>,
    store_parameter: Option<String>,
    product_owner: Option<products::Scope>,
    products: products::Products,
    retained_stores: Vec<String>,
    inferred_state_values: BTreeSet<String>,
    has_store_templates: bool,
    has_single_state_values: bool,
    default_functions: usize,
    nominal_binder_collision: bool,
    signatures: BTreeMap<String, Cow<'a, Function>>,
    foreign_signatures: BTreeMap<&'a str, (&'a str, &'a Function)>,
    instances: BTreeMap<(String, Vec<String>), String>,
    realized_impls: BTreeSet<(trait_instances::Key, String)>,
    realized_references: Vec<(trait_instances::Reference, TypeRef)>,
    impl_definitions: BTreeMap<(usize, usize), String>,
    instance_types: BTreeMap<String, TypeRef>,
    anonymous_function: Option<String>,
    anonymous_substitutions: BTreeMap<String, BTreeMap<String, TypeRef>>,
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
        ItemKind::Function(f) if !super::lifetimes::only_life(f) => &f.generics,
        ItemKind::Aggregate(a) => &a.generics,
        ItemKind::Enum(e) => &e.generics,
        _ => &[],
    }
}
fn type_parameters(function: &Function) -> impl Iterator<Item = &GenericParam> + Clone {
    function
        .generics
        .iter()
        .filter(|parameter| matches!(parameter.kind, GenericParamKind::Type(_)))
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
            trait_calls: trait_instances::Calls::new(),
            templates: BTreeMap::new(),
            enums: BTreeMap::new(),
            nominals: BTreeMap::new(),
            return_type: None,
            callable_name: None,
            callable_pair: None,
            active_callables: None,
            store_parameter: None,
            product_owner: None,
            products: products::Products::default(),
            retained_stores: Vec::new(),
            inferred_state_values: BTreeSet::new(),
            has_store_templates: false,
            has_single_state_values: false,
            default_functions: 0,
            nominal_binder_collision: false,
            signatures: BTreeMap::new(),
            foreign_signatures: BTreeMap::new(),
            instances: BTreeMap::new(),
            realized_impls: BTreeSet::new(),
            realized_references: Vec::new(),
            impl_definitions: BTreeMap::new(),
            instance_types: BTreeMap::new(),
            anonymous_function: None,
            anonymous_substitutions: BTreeMap::new(),
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
        engine.validate_impl_recipes()?;
        engine.validate_trait_defaults(module)?;
        if engine.nominal_binder_collision {
            return Ok(engine);
        }
        // Validate the parametric subset even when a template is not instantiated.
        if engine.budget.has_enum_profiles() {
            for item in &module.items {
                engine.budget.node(item.span, 0)?;
                if let ItemKind::Enum(declaration) = &item.kind
                    && engine.budget.enum_profile(item.span)
                {
                    record_enums::validate(
                        declaration,
                        &engine.nominals,
                        &engine.traits,
                        &mut engine.budget,
                    )?;
                }
            }
        }
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
                if matches!(&item.kind, ItemKind::Function(f) if super::lifetimes::copy_life(f))
                    && matches!(parameter.kind, GenericParamKind::Lifetime)
                {
                    continue;
                }
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
                    } else if matches!(
                        parameter.kind,
                        GenericParamKind::Type(_) | GenericParamKind::Const(_)
                    ) {
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
                    GenericParamKind::Const(ty) => {
                        consts::domain(ty)?;
                    }
                    GenericParamKind::Type(bounds)
                        if bounds.is_empty()
                            || callable.is_some()
                            || bounds.iter().all(stores::is_bound)
                            || matches!(&item.kind, ItemKind::Aggregate(a) if a.kind == AggregateKind::Struct)
                                && bounds.iter().all(copy::is_bound)
                            || matches!(item.kind, ItemKind::Enum(_))
                                && bounds.iter().all(copy::is_bound)
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
                let checked = consts::definition(f, &mut engine.budget)?;
                validate_parametric(
                    &checked,
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
                    &mut engine.trait_calls,
                )?;
            }
            if let ItemKind::Enum(e) = &item.kind
                && !engine.budget.enum_profile(item.span)
                && e.name != options::TEMPLATE
            {
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
    fn run(&mut self, module: &Module, products: &mut products::Products) -> R<Module> {
        self.products.trait_instances = self.traits.has_instances;
        self.products.record_variants = self.budget.has_enum_profiles();
        self.products.protected_binders = self.budget.has_primitive_binders();
        self.collect_products(module)?;
        let mut result = Module {
            id: module.id,
            span: module.span,
            items: vec![],
        };
        for item in &module.items {
            if matches!(&item.kind, ItemKind::Impl(i) if i.trait_type.is_some() && (!i.generics.is_empty() || !i.where_clause.is_empty() || trait_bounds::plain(&i.target).is_none()))
            {
                continue;
            }
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
        self.products.allowance = Some(self.budget.remaining_limits());
        *products = std::mem::take(&mut self.products);
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
        let product =
            matches!(&item.kind, ItemKind::Aggregate(a) if products::parameter(a).is_some());
        let product_arguments = if product {
            for argument in &arguments {
                self.budget.ty(argument)?;
            }
            Some(arguments.clone())
        } else {
            None
        };
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
            .filter(|p| !matches!(p.kind, GenericParamKind::Lifetime))
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
        let arguments = parameters
            .iter()
            .zip(arguments)
            .map(|(parameter, actual)| {
                if matches!(parameter.kind, GenericParamKind::Const(_))
                    || consts::expression(&actual).is_some()
                {
                    consts::normalize(parameter, &actual, &mut self.budget)
                } else {
                    Ok(actual)
                }
            })
            .collect::<R<Vec<_>>>()?;
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
        let product_scope = if product {
            self.product_scope(&arguments, span)?
        } else {
            None
        };
        if let Some(scope) = &product_scope {
            self.budget.text(&scope.owner, span)?;
            key.1.push(format!("@product-owner:{}", scope.owner));
        }
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
            .saturating_add(self.traits.instance_count())
            .saturating_add(self.default_functions)
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
        let mut product_bounds = trait_bounds::Bounds::new();
        let mut product_variables = BTreeSet::new();
        if product && let Some(name) = &self.store_parameter {
            self.budget.node(span, 0)?;
            self.budget.text(name, span)?;
            product_bounds.insert(name.clone(), vec![trait_bounds::Bound::StoreElement]);
            product_variables.insert(name.clone());
        }
        self.traits.check_arguments(
            params(item),
            &substitution,
            (
                &product_bounds,
                &product_variables,
                callable::StoreCallbacks::from_names(
                    self.callable_name.as_deref(),
                    self.callable_pair.as_ref(),
                ),
            ),
            &mut self.budget,
            span,
            retained.as_ref(),
        )?;
        self.realize_bounds(params(item), &substitution, span)?;
        self.budget.substituted_item(item, &substitution)?;
        consts::prepay_scopes(item, &substitution, &mut self.budget)?;
        let mut instance = item.clone();
        if let ItemKind::Function(f) = &item.kind
            && let Some(calls) = self.trait_calls.get(&f.name)
        {
            trait_instances::rewrite_calls(&mut instance, calls, &substitution, &mut self.budget)?;
        }
        substitute_item(&mut instance, &substitution)?;
        self.budget.item(&instance)?;
        // Original generic enums retain their existing Copy payload profile,
        // even though ordinary monomorphic enums now admit owners. Inspect the
        // actual substituted fields while the real template identity exists.
        if let ItemKind::Enum(e) = &instance.kind
            && template != options::TEMPLATE
        {
            for variant in &e.variants {
                if let VariantFields::Tuple(fields) = &variant.fields {
                    for field in fields {
                        if field.manages {
                            return Err(unsupported(field.span, "owned generic enum payload"));
                        }
                        copy::concrete_storage(&field.ty, &self.nominals, &mut self.budget)?;
                    }
                } else if let VariantFields::Record(fields) = &variant.fields {
                    for field in fields {
                        self.budget.node(field.span, 0)?;
                        copy::concrete_storage(&field.ty, &self.nominals, &mut self.budget)?;
                    }
                }
            }
        }
        match &mut instance.kind {
            ItemKind::Function(f) => {
                f.generics.retain(|parameter| {
                    matches!(parameter.kind, GenericParamKind::Lifetime)
                        || retained
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
        if let Some(arguments) = product_arguments {
            self.record_product(template, &generated, arguments, product_scope, span)?;
        }
        if self.budget.has_anonymous_sums() && matches!(instance.kind, ItemKind::Function(_)) {
            self.budget.text(&generated, span)?;
            self.budget.node(span, 0)?;
            // Transfer the already bounded substitution map; no second clone.
            self.anonymous_substitutions
                .insert(generated.clone(), substitution);
        }
        self.pending.push_back(instance);
        Ok(store_templates::Instantiation::Eager(generated))
    }
    fn ty(&mut self, ty: &mut TypeRef, depth: usize) -> R<()> {
        self.tick(ty.span, depth)?;
        if self.traits.has_associated && trait_bounds::associated::parts(ty).is_some() {
            *ty = self.traits.normalize_associated(
                ty,
                &trait_bounds::Bounds::new(),
                &BTreeSet::new(),
                &mut self.budget,
            )?;
            return self.ty(ty, depth + 1);
        }
        match &mut ty.kind {
            TypeKind::Named { path, args } => {
                let mut values = Vec::new();
                let declaration = (path.len() == 1)
                    .then(|| self.templates.get(&path[0]).copied())
                    .flatten();
                for (index, arg) in args.iter_mut().enumerate() {
                    let parameter = declaration.and_then(|item| params(item).get(index));
                    if let GenericArg::Type(value) = arg
                        && !parameter.is_some_and(|p| matches!(p.kind, GenericParamKind::Const(_)))
                    {
                        self.ty(value, depth + 1)?;
                        values.push(value.clone());
                    } else {
                        values.push(consts::argument(arg, parameter, &mut self.budget)?);
                    }
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
                let saved_anonymous = self.anonymous_function.take();
                if self.anonymous_substitutions.contains_key(&f.name) {
                    self.budget.text(&f.name, f.span)?;
                    self.anonymous_function = Some(f.name.clone());
                }
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
                let saved_product_owner = self.product_owner.take();
                if !self.products.is_empty()
                    && (self.store_parameter.is_some()
                        || self.callable_name.is_some()
                        || self.callable_pair.is_some())
                {
                    self.budget.node(f.span, 0)?;
                    self.budget.text(&f.name, f.span)?;
                    for name in self
                        .store_parameter
                        .iter()
                        .chain(self.callable_name.iter())
                        .chain(self.callable_pair.iter().flatten())
                    {
                        self.budget.text(name, f.span)?;
                    }
                    self.product_owner = Some(products::Scope {
                        owner: f.name.clone(),
                        span: f.span,
                        ordinary: self.store_parameter.clone(),
                        callable: self.callable_name.clone(),
                        pair: self.callable_pair.clone(),
                    });
                }
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
                let saved_callables = std::mem::replace(&mut self.active_callables, callable);
                match &mut f.body {
                    FunctionBody::Block(body) => self.block(body, &mut env, 0)?,
                    FunctionBody::Expression(value) => {
                        let expected = self.return_type.clone();
                        self.expr_expected(value, &mut env, expected.as_ref(), 0)?;
                    }
                    FunctionBody::Declaration => {}
                }
                self.return_type = saved_result;
                self.active_callables = saved_callables;
                self.callable_name = saved_callable;
                self.callable_pair = saved_pair;
                self.store_parameter = saved_store;
                self.product_owner = saved_product_owner;
                self.anonymous_function = saved_anonymous;
            }
            ItemKind::Aggregate(a) => {
                let saved = (
                    self.product_owner.clone(),
                    self.store_parameter.clone(),
                    self.callable_name.clone(),
                    self.callable_pair.clone(),
                );
                if let Some(scope) = self.product_aggregate_scope(&a.name, item.span)? {
                    // Restoring the engine's lexical view creates a second
                    // copy beside the retained application owner context.
                    for name in scope
                        .ordinary
                        .iter()
                        .chain(scope.callable.iter())
                        .chain(scope.pair.iter().flatten())
                    {
                        self.budget.text(name, item.span)?;
                    }
                    self.store_parameter = scope.ordinary.clone();
                    self.callable_name = scope.callable.clone();
                    self.callable_pair = scope.pair.clone();
                    self.product_owner = Some(scope);
                }
                for field in &mut a.fields {
                    self.ty(&mut field.ty, 0)?;
                    if let Some(value) = &mut field.default {
                        self.expr(value, &mut Env::new(), 0)?;
                    }
                }
                for item in &mut a.methods {
                    self.item(item)?;
                }
                (
                    self.product_owner,
                    self.store_parameter,
                    self.callable_name,
                    self.callable_pair,
                ) = saved;
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
                self.complete_trait_defaults(i, item.span)?;
                let source_target = if self.traits.has_associated {
                    self.budget.ty(&i.target)?;
                    self.budget.text("Self", item.span)?;
                    let substitutions = BTreeMap::from([("Self".into(), i.target.clone())]);
                    for member in &mut i.members {
                        self.budget.substituted_item(member, &substitutions)?;
                        substitute_item(member, &substitutions)?;
                    }
                    self.budget.ty(&i.target)?;
                    Some(i.target.clone())
                } else {
                    None
                };
                self.ty(&mut i.target, 0)?;
                if self.traits.has_instances
                    && let Some(application) = &i.trait_type
                    && trait_instances::head(application)
                        .is_some_and(|name| !super::static_traits::reserved_trait(name))
                {
                    let reference = self.traits.reference(application, &mut self.budget)?;
                    let (_, declaration) = self.traits.declaration_item(&reference);
                    if !declaration.generics.is_empty() || self.traits.has_associated {
                        let mut implemented = BTreeSet::new();
                        for member in &i.members {
                            let ItemKind::Function(actual) = &member.kind else {
                                continue;
                            };
                            self.budget.text(&actual.name, member.span)?;
                            self.budget.node(member.span, 0)?;
                            if !implemented.insert(actual.name.as_str()) {
                                return Err(Diagnostic::error(
                                    Phase::Expand,
                                    "E-TRAIT-METHOD-SET",
                                    Some(member.span),
                                    "duplicate implementation method",
                                ));
                            }
                            let mut required = None;
                            for candidate in &declaration.members {
                                self.budget.node(candidate.span, 0)?;
                                if let ItemKind::Function(method) = &candidate.kind {
                                    self.budget.text(&method.name, candidate.span)?;
                                    self.budget.text(&actual.name, member.span)?;
                                    if method.name == actual.name {
                                        required = Some(candidate);
                                        break;
                                    }
                                }
                            }
                            if let Some(required) = required {
                                if self.traits.has_associated && declaration.generics.is_empty() {
                                    self.budget.item(required)?;
                                }
                                let mut signature = trait_instances::signature(
                                    declaration,
                                    required,
                                    &reference,
                                    &mut self.budget,
                                )?
                                .into_owned();
                                if let Some(source_target) = &source_target {
                                    self.budget.text("Self", member.span)?;
                                    self.budget.ty(source_target)?;
                                    let substitutions =
                                        BTreeMap::from([("Self".into(), source_target.clone())]);
                                    for p in &mut signature.params {
                                        self.budget.substituted_type(&p.ty, &substitutions)?;
                                        Substitution {
                                            substitutions: &substitutions,
                                        }
                                        .ty(&mut p.ty);
                                    }
                                    if let Some(t) = &mut signature.return_type {
                                        self.budget.substituted_type(t, &substitutions)?;
                                        Substitution {
                                            substitutions: &substitutions,
                                        }
                                        .ty(t);
                                    }
                                }
                                for p in &mut signature.params {
                                    self.ty(&mut p.ty, 0)?;
                                }
                                if let Some(t) = &mut signature.return_type {
                                    self.ty(t, 0)?;
                                }
                                let key = trait_instances::copy_key(
                                    &reference.key,
                                    &mut self.budget,
                                    member.span,
                                )?;
                                let host = trait_bounds::plain(&i.target).ok_or_else(|| {
                                    unsupported(
                                        i.target.span,
                                        "trait impl requires a concrete non-generic Self",
                                    )
                                })?;
                                self.budget.text(host, member.span)?;
                                self.budget.node(member.span, 0)?;
                                let expected = trait_instances::ConcreteSignature {
                                    key,
                                    host: host.to_owned(),
                                    method: signature,
                                };
                                let identity_key = trait_instances::copy_key(
                                    &expected.key,
                                    &mut self.budget,
                                    member.span,
                                )?;
                                self.budget.text(host, member.span)?;
                                self.budget.text(&actual.name, member.span)?;
                                if self
                                    .products
                                    .trait_signatures
                                    .insert(
                                        (identity_key, host.to_owned(), actual.name.clone()),
                                        expected,
                                    )
                                    .is_some()
                                {
                                    return Err(Diagnostic::error(
                                        Phase::Expand,
                                        "E-TRAIT-DUPLICATE",
                                        Some(member.span),
                                        "duplicate implementation of the same trait instance and Self",
                                    ));
                                }
                            }
                        }
                    }
                }
                for item in &mut i.members {
                    self.item(item)?;
                }
                if self.traits.has_associated {
                    i.members
                        .retain(|member| !matches!(member.kind, ItemKind::TypeAlias(_)));
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
        self.value_hint(e, env, depth, false)
    }
    fn value_hint(
        &mut self,
        e: &Expr,
        env: &Env,
        depth: usize,
        default_integer: bool,
    ) -> R<Option<TypeRef>> {
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
                Ok(["u8", "u16", "u64", "usize", "i32", "i64"]
                    .into_iter()
                    .find(|suffix| text.ends_with(suffix))
                    .or(default_integer.then_some("i32"))
                    .map(|suffix| named(suffix, e.span)))
            }
            ExprKind::Literal(Literal::Float(text)) => {
                self.budget.text(text, e.span)?;
                Ok(["f32", "f64"]
                    .into_iter()
                    .find(|suffix| {
                        text.ends_with(suffix)
                            || (suffix == &"f32" && text.starts_with("@bits:f32:"))
                            || (suffix == &"f64" && text.starts_with("@bits:f64:"))
                    })
                    .or(default_integer.then_some("f64"))
                    .map(|suffix| named(suffix, e.span)))
            }
            ExprKind::Literal(Literal::Bool(_)) => Ok(Some(named("bool", e.span))),
            ExprKind::Group(inner) | ExprKind::MetaEval(inner) => {
                self.value_hint(inner, env, depth + 1, default_integer)
            }
            ExprKind::Unary {
                op: UnaryOp::Not, ..
            } => Ok(Some(named("bool", e.span))),
            ExprKind::Unary {
                op: UnaryOp::Dereference,
                operand,
            } => {
                let Some(ty) = self.value_hint(operand, env, depth + 1, default_integer)? else {
                    return Ok(None);
                };
                let TypeKind::Reference { inner, .. } = &grouped(&ty).kind else {
                    return Ok(None);
                };
                self.budget.ty(inner)?;
                Ok(Some(*inner.clone()))
            }
            ExprKind::Unary { operand, .. } => {
                self.value_hint(operand, env, depth + 1, default_integer)
            }
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
            ExprKind::Binary { left, right, .. } => {
                if check::uncontextual_float(left, &mut self.budget, depth + 1)?
                    && let Some(ty) = self
                        .value_hint(right, env, depth + 1, false)?
                        .filter(check::floating)
                {
                    return Ok(Some(ty));
                }
                match self.value_hint(left, env, depth + 1, default_integer)? {
                    Some(ty) => Ok(Some(ty)),
                    None => self.value_hint(right, env, depth + 1, default_integer),
                }
            }
            ExprKind::If {
                then_value,
                else_value,
                ..
            } => match self.value_hint(then_value, env, depth + 1, default_integer)? {
                Some(ty) => Ok(Some(ty)),
                None => self.value_hint(else_value, env, depth + 1, default_integer),
            },
            ExprKind::Try(operand) => {
                let Some(ty) = self.value_hint(operand, env, depth + 1, default_integer)? else {
                    return Ok(None);
                };
                let TypeKind::Error { result, .. } = &grouped(&ty).kind else {
                    return Ok(None);
                };
                self.budget.ty(result)?;
                Ok(Some(*result.clone()))
            }
            ExprKind::Index { object, .. } | ExprKind::Member { object, .. } => {
                let Some(ty) = self.value_hint(object, env, depth + 1, default_integer)? else {
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
                            member:
                                MemberName::Name(member) | MemberName::Constrained { name: member, .. },
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
                        && self
                            .value_hint(object, env, depth + 1, default_integer)?
                            .is_some_and(|ty| {
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
                    if default_integer && let Some(ty) = local {
                        self.budget.ty(ty)?;
                    }
                    let mut result = local.as_ref().and_then(|ty| match &grouped(ty).kind {
                        TypeKind::Function { result, .. } => Some(result.as_ref()),
                        _ => callable::opaque_signature(ty).map(|(_, _, result)| result),
                    });
                    if default_integer
                        && result.is_none()
                        && let Some(ty) = local
                        && let Some(bounds) = &self.active_callables
                    {
                        for bound in bounds.bindings() {
                            self.budget.ty(&bound.result)?;
                            self.budget.text(&bound.generic_name, e.span)?;
                            if callable::parameter_mode(ty, &bound.generic_name).is_some() {
                                result = Some(&bound.result);
                                break;
                            }
                        }
                    }
                    if let Some(result) = result {
                        self.budget.ty(result)?;
                    }
                    return Ok(result.cloned());
                }
                if default_integer && let Some(function) = self.signatures.get(target) {
                    let mut variables = BTreeSet::new();
                    for parameter in type_parameters(function) {
                        self.budget.text(&parameter.name, e.span)?;
                        variables.insert(parameter.name.clone());
                    }
                    let Some(result) = &function.return_type else {
                        return Ok(Some(check::unit(e.span)));
                    };
                    self.budget.ty(result)?;
                    if !check::dependent(result, &variables)
                        && callable::opaque_signature(result).is_none()
                    {
                        // Its declared result already determines the hint.
                        // This lookahead must not type-check nested arguments;
                        // the real source-order pass checks their injections.
                        return Ok(Some(result.clone()));
                    }
                }
                let mut actuals = Vec::new();
                for arg in arguments {
                    actuals.push(self.value_hint(&arg.value, env, depth + 1, default_integer)?);
                }
                let Some(function) = self.signatures.get(target) else {
                    return Ok(None);
                };
                let Some(result) = &function.return_type else {
                    return Ok(Some(check::unit(e.span)));
                };
                let mut variables = BTreeSet::new();
                for parameter in type_parameters(function) {
                    self.budget.text(&parameter.name, e.span)?;
                    variables.insert(parameter.name.clone());
                }
                self.budget.ty(result)?;
                let mut found = BTreeMap::new();
                if let Some(arguments) = explicit {
                    if arguments.len() != type_parameters(function).count() {
                        return Ok(None);
                    }
                    for (parameter, argument) in type_parameters(function).zip(arguments) {
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
        let anonymous_context =
            expected.filter(|ty| matches!(&grouped(ty).kind, TypeKind::Union(_)));
        let expected = expected.filter(|ty| !matches!(&grouped(ty).kind, TypeKind::Union(_)));
        // A surrounding E!T admits either exact payload or the whole union.
        // It therefore cannot constrain a generic result variable to E!T
        // before argument inference determines the expression's own type.
        let error_context =
            expected.filter(|ty| matches!(grouped(ty).kind, TypeKind::Error { .. }));
        let store_success_context = expected.filter(|ty| stores::success(ty).is_some());
        let expected = expected.filter(|ty| !matches!(grouped(ty).kind, TypeKind::Error { .. }));
        if let Some(ty) = self.enum_expression(e, env, expected.or(error_context), depth)? {
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
                        } else if text.ends_with("u64") {
                            "u64"
                        } else if text.ends_with("usize") {
                            "usize"
                        } else if text.ends_with("i64") {
                            "i64"
                        } else {
                            "i32"
                        },
                        e.span,
                    ))
                }
            }
            ExprKind::Literal(Literal::Float(text)) => {
                let suffix = ["f32", "f64"].into_iter().find(|suffix| {
                    text.ends_with(suffix)
                        || (suffix == &"f32" && text.starts_with("@bits:f32:"))
                        || (suffix == &"f64" && text.starts_with("@bits:f64:"))
                });
                if suffix.is_none() && expected.is_some_and(check::floating) {
                    self.budget.ty(expected.unwrap())?;
                    expected.cloned()
                } else {
                    Some(named(suffix.unwrap_or("f64"), e.span))
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
                self.expr_expected(inner, env, expected.or(error_context), depth + 1)?
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
                let float_hint = if hint.is_none()
                    && check::uncontextual_float(left, &mut self.budget, depth + 1)?
                {
                    self.value_hint(right, env, depth + 1, false)?
                        .filter(check::floating)
                } else {
                    None
                };
                let hint = hint.or(float_hint.as_ref());
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
                let mut trait_signature = None;
                let mut type_qualified = false;
                if self.traits.has_instances
                    && let ExprKind::Member {
                        object,
                        member:
                            MemberName::Name(member) | MemberName::Constrained { name: member, .. },
                    } = &mut callee.kind
                {
                    let mut hint = self.value_hint(object, env, depth + 1, false)?;
                    if hint.is_none()
                        && let ExprKind::Name(name) = &object.kind
                        && !env.contains_key(name)
                        && self.nominals.contains_key(name)
                    {
                        hint = Some(named(name, object.span));
                        type_qualified = true;
                    }
                    if let Some(actual) = hint {
                        let actual = self.original_type(&actual)?;
                        if let Some(host) = trait_bounds::plain(projection_shape(&actual))
                            && let Some((target, signature)) = self.traits.concrete_method(
                                host,
                                member,
                                &mut self.budget,
                                callee.span,
                            )?
                        {
                            self.budget.text(&target, callee.span)?;
                            member.clone_from(&target);
                            trait_signature = Some(signature);
                        }
                        if trait_signature.is_none()
                            && member.starts_with("$trait_")
                            && let Some(signature) = self.realized_method(
                                projection_shape(&actual),
                                member,
                                callee.span,
                            )?
                        {
                            trait_signature = Some(signature);
                        }
                        if trait_signature.is_none()
                            && !self.traits.recipes.is_empty()
                            && let Some((target, signature, witness)) = self.traits.recipe_method(
                                projection_shape(&actual),
                                member,
                                &mut self.budget,
                                callee.span,
                            )?
                        {
                            self.realize_witness(witness)?;
                            self.budget.text(&target, callee.span)?;
                            member.clone_from(&target);
                            trait_signature = Some(signature);
                        }
                    }
                }
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
                let mut contextual_constraints = false;
                for index in 0..arguments.len() {
                    let mut hint = hints
                        .as_ref()
                        .map(|h| h.hint(index, &mut self.budget))
                        .transpose()?
                        .flatten();
                    if hint.is_none()
                        && hints
                            .as_ref()
                            .map(|h| h.unresolved_context(index, &mut self.budget))
                            .transpose()?
                            .unwrap_or(false)
                    {
                        contextual_constraints = true;
                        for union_pass in [false, true] {
                            for (other_index, argument) in arguments.iter().enumerate() {
                                self.budget.node(argument.span, depth + 1)?;
                                if !hints
                                    .as_ref()
                                    .expect("contextual call hints")
                                    .independent_input(other_index, union_pass, &mut self.budget)?
                                {
                                    continue;
                                }
                                if let Some(ty) =
                                    self.value_hint(&argument.value, env, depth + 1, true)?
                                {
                                    let ty = self.original_type(&ty)?;
                                    hints
                                        .as_mut()
                                        .expect("contextual call hints")
                                        .independent_constraint(
                                            other_index,
                                            &ty,
                                            argument.span,
                                            &mut self.budget,
                                        )?;
                                }
                            }
                        }
                        hint = hints
                            .as_ref()
                            .map(|h| h.hint(index, &mut self.budget))
                            .transpose()?
                            .flatten();
                    }
                    if hint.is_none()
                        && let Some(signature) = &trait_signature
                    {
                        let skip = usize::from(
                            !type_qualified
                                && signature.params.first().is_some_and(|p| p.name == "self"),
                        );
                        if let Some(parameter) = signature.params.get(index + skip) {
                            self.budget.ty(&parameter.ty)?;
                            hint = Some(parameter.ty.clone());
                        }
                    }
                    let arg = &mut arguments[index];
                    let ty = self.expr_expected(&mut arg.value, env, hint.as_ref(), depth + 1)?;
                    let ty = if ty.is_none()
                        && hint
                            .as_ref()
                            .is_some_and(|ty| matches!(grouped(ty).kind, TypeKind::Error { .. }))
                    {
                        self.value_hint(&arg.value, env, depth + 1, true)?
                    } else {
                        ty
                    };
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
                        let declaration = if let ExprKind::Name(name) = &base.kind {
                            self.templates.get(name).copied()
                        } else {
                            None
                        };
                        for (index, arg) in args.iter_mut().enumerate() {
                            let parameter = declaration.and_then(|item| params(item).get(index));
                            if let GenericArg::Type(t) = arg
                                && !parameter
                                    .is_some_and(|p| matches!(p.kind, GenericParamKind::Const(_)))
                            {
                                self.ty(t, depth + 1)?;
                                types.push(t.clone());
                            } else {
                                types.push(consts::argument(arg, parameter, &mut self.budget)?);
                            }
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
                                    type_parameters(function).map(|p| p.name.clone()).collect();
                                let mut found = if contextual_constraints {
                                    hints
                                        .as_ref()
                                        .expect("contextual call hints")
                                        .context_constraints(&mut self.budget, e.span)?
                                } else {
                                    BTreeMap::new()
                                };
                                if let (Some(result), Some(expected)) =
                                    (&function.return_type, expected)
                                    && (!self.traits.has_associated
                                        || !trait_bounds::associated::contains(
                                            result,
                                            &mut self.budget,
                                        )?)
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
                                type_parameters(function).map(|p| p.name.clone()).collect();
                            let mut found = if contextual_constraints {
                                hints
                                    .as_ref()
                                    .expect("contextual call hints")
                                    .context_constraints(&mut self.budget, e.span)?
                            } else {
                                BTreeMap::new()
                            };
                            if let (Some(result), Some(expected)) =
                                (&function.return_type, expected)
                                && (!self.traits.has_associated
                                    || !trait_bounds::associated::contains(
                                        result,
                                        &mut self.budget,
                                    )?)
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
                            type_parameters(function)
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
                    let callee_type = self.expr(callee, env, depth + 1)?;
                    if let Some(mut signature) = trait_signature {
                        if let Some(result) = &mut signature.return_type {
                            self.ty(result, depth + 1)?;
                        }
                        signature.return_type
                    } else {
                        // A dynamic call still has a statically known complete
                        // signature. Preserve its result for following source
                        // patterns; Types checks the actual call independently.
                        match callee_type.as_ref().map(grouped).map(|ty| &ty.kind) {
                            Some(TypeKind::Function { result, .. }) => {
                                self.budget.ty(result)?;
                                Some(*result.clone())
                            }
                            _ => None,
                        }
                    }
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
                if let MemberName::Constrained { owner, .. } = member {
                    self.ty(owner, depth + 1)?;
                }
                let object_type = self.expr(object, env, depth + 1)?;
                match (object_type.as_ref().map(projection_shape), member) {
                    (
                        Some(TypeRef {
                            kind: TypeKind::Tuple(fields),
                            ..
                        }),
                        MemberName::TupleIndex(i),
                    ) => fields.get(*i as usize).cloned(),
                    (
                        Some(ty),
                        MemberName::Name(member) | MemberName::Constrained { name: member, .. },
                    ) => self.record_field(ty, member)?,
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
                let ty =
                    self.expr_expected(then_value, env, expected.or(error_context), depth + 1)?;
                // The erased E!R expectation does not make its first payload
                // the expected nominal type of a distinct enum alternative.
                // Preserve the old same-nominal/context-free path exactly.
                let else_constructor = match &else_value.kind {
                    ExprKind::Call { callee, .. } => callee.as_ref(),
                    _ => else_value.as_ref(),
                };
                let alternative = if let (Some(union), Some(first), Some((name, _, _))) = (
                    error_context,
                    ty.as_ref(),
                    enums::constructor(else_constructor),
                ) && matches!(&grouped(first).kind,TypeKind::Named{path,..} if path.as_slice()!=[name])
                    && let TypeKind::Error { error, result } = &grouped(union).kind
                {
                    [error.as_ref(),result.as_ref()].into_iter().find(|payload|
                        matches!(&grouped(payload).kind,TypeKind::Named{path,..} if path.as_slice()==[name]))
                        .or(error_context)
                } else {
                    anonymous_context.or(expected).or(ty.as_ref())
                };
                self.expr_expected(else_value, env, alternative, depth + 1)?;
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
                        GenericArg::Const(e) => {
                            budget.const_expression(e)?;
                            Ok(GenericArg::Const(e.clone()))
                        }
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
        TypeKind::Union(ts) => TypeKind::Union(
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
    consts::length(length)
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
    type_key_bounded(ty, budget)
}
fn type_key_bounded(ty: &TypeRef, budget: &mut Budget<'_>) -> R<String> {
    // The profile is recorded during the existing bounded AST walk. Requests
    // without anonymous sums retain their original key fees and traversal.
    if budget.has_anonymous_sums() {
        type_keys::prepay(ty, budget)?;
    }
    render_type_key(ty)
}
fn render_type_key(ty: &TypeRef) -> R<String> {
    Ok(match &ty.kind {
        TypeKind::Named { path, args } if path.as_slice() == ["void"] && args.is_empty() => {
            "()".into()
        }
        TypeKind::Named { path, args } => {
            let mut key = format!("N{}:{}", path.join(".").len(), path.join("."));
            for a in args {
                let argument = match a {
                    GenericArg::Type(t) => render_type_key(t)?,
                    GenericArg::Const(e) => format!("#{}", consts::key(e)?),
                };
                key.push_str(&format!("<{argument}>"));
            }
            key
        }
        TypeKind::Group(t) => return render_type_key(t),
        TypeKind::Reference { mutable, inner, .. } => {
            format!("R{mutable}<{}>", render_type_key(inner)?)
        }
        TypeKind::Union(ts) => {
            fn members(ty: &TypeRef, out: &mut Vec<String>) -> R<()> {
                if let TypeKind::Union(parts) = &grouped(ty).kind {
                    for part in parts {
                        members(part, out)?;
                    }
                } else {
                    out.push(render_type_key(ty)?);
                }
                Ok(())
            }
            let mut keys = Vec::new();
            for member in ts {
                members(member, &mut keys)?;
            }
            keys.sort();
            format!(
                "U{}:{}",
                keys.len(),
                keys.iter()
                    .map(|k| format!("{}:{k}", k.len()))
                    .collect::<String>()
            )
        }
        TypeKind::Tuple(ts) => format!(
            "({})",
            ts.iter()
                .map(render_type_key)
                .collect::<R<Vec<_>>>()?
                .join(",")
        ),
        TypeKind::Array {
            element,
            length: Some(length),
        } => format!(
            "A{}<{}>",
            consts::length_key(length, &BTreeMap::new())?,
            render_type_key(element)?
        ),
        TypeKind::MetaEval(expression) => format!("#{}", consts::key(expression)?),
        TypeKind::Array {
            element,
            length: None,
        } => format!("S<{}>", render_type_key(element)?),
        TypeKind::Optional(t) => {
            format!("?{}", render_type_key(t)?)
        }
        TypeKind::Error { error, result } => format!(
            "E<{}><{}>",
            render_type_key(error)?,
            render_type_key(result)?
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
                .map(render_type_key)
                .collect::<R<Vec<_>>>()?
                .join(","),
            render_type_key(result)?
        ),
        TypeKind::Function { params, result } => format!(
            "F({})->{}",
            params
                .iter()
                .map(render_type_key)
                .collect::<R<Vec<_>>>()?
                .join(","),
            render_type_key(result)?
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
    unify_bounded(pattern, actual, variables, found, span, budget)
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
    if context_argument(pattern, actual, variables, found, span, budget)? {
        return Ok(());
    }
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
        return unify_bounded(expected, actual, variables, found, span, budget);
    }
    unify_bounded(pattern, actual, variables, found, span, budget)
}
// Only an independently complete parameter union can select an injection.
// Missing variables still use ordinary whole-type unification, never two
// alternative unifications followed by a choice of the successful branch.
fn context_argument(
    pattern: &TypeRef,
    actual: &TypeRef,
    variables: &BTreeSet<String>,
    found: &BTreeMap<String, TypeRef>,
    span: Span,
    budget: &mut Budget<'_>,
) -> R<bool> {
    if !matches!(
        grouped(pattern).kind,
        TypeKind::Error { .. } | TypeKind::Union(_)
    ) {
        return Ok(false);
    }
    if context_unresolved(pattern, variables, found, budget)? {
        return Ok(false);
    }
    let expected = enums::substituted(pattern, found, budget)?;
    check::same_return(actual, &expected, span, budget)?;
    Ok(true)
}
fn context_unresolved(
    pattern: &TypeRef,
    variables: &BTreeSet<String>,
    found: &BTreeMap<String, TypeRef>,
    budget: &mut Budget<'_>,
) -> R<bool> {
    for name in variables {
        budget.text(name, pattern.span)?;
        if !found.contains_key(name) {
            budget.ty(pattern)?;
            if enums::contains(pattern, name) {
                return Ok(true);
            }
        }
    }
    Ok(false)
}
fn independent_input(
    pattern: &TypeRef,
    variables: &BTreeSet<String>,
    found: &BTreeMap<String, TypeRef>,
    union_pass: bool,
    budget: &mut Budget<'_>,
) -> R<bool> {
    budget.ty(pattern)?;
    if matches!(
        grouped(pattern).kind,
        TypeKind::Error { .. } | TypeKind::Union(_)
    ) != union_pass
    {
        return Ok(false);
    }
    if union_pass {
        context_unresolved(pattern, variables, found, budget)
    } else {
        Ok(true)
    }
}
fn unify_bounded(
    pattern: &TypeRef,
    actual: &TypeRef,
    variables: &BTreeSet<String>,
    found: &mut BTreeMap<String, TypeRef>,
    span: Span,
    budget: &mut Budget<'_>,
) -> R<()> {
    if let TypeKind::Group(t) = &pattern.kind {
        return unify_bounded(t, actual, variables, found, span, budget);
    }
    if let TypeKind::Group(t) = &actual.kind {
        return unify_bounded(pattern, t, variables, found, span, budget);
    }
    if let TypeKind::Named { path, args } = &pattern.kind
        && path.len() == 1
        && args.is_empty()
        && variables.contains(&path[0])
    {
        if let Some(prior) = found.get(&path[0]) {
            if type_key_bounded(prior, budget)? != type_key_bounded(actual, budget)? {
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
            unify_bounded(a, b, variables, found, span, budget)?;
            unify_bounded(x, y, variables, found, span, budget)
        }
        (TypeKind::Named { path: a, args: x }, TypeKind::Named { path: b, args: y })
            if a == b && x.len() == y.len() =>
        {
            for (a, b) in x.iter().zip(y) {
                let (GenericArg::Type(a), GenericArg::Type(b)) = (a, b) else {
                    return Err(unsupported(span, "const generic unification"));
                };
                unify_bounded(a, b, variables, found, span, budget)?;
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
        ) if a == b => unify_bounded(x, y, variables, found, span, budget),
        (TypeKind::Tuple(a), TypeKind::Tuple(b)) if a.len() == b.len() => {
            for (a, b) in a.iter().zip(b) {
                unify_bounded(a, b, variables, found, span, budget)?;
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
        ) if consts::length_key(x, found)? == consts::length_key(y, &BTreeMap::new())? => {
            unify_bounded(a, b, variables, found, span, budget)
        }
        (
            TypeKind::Array {
                element: a,
                length: None,
            },
            TypeKind::Array {
                element: b,
                length: None,
            },
        ) => unify_bounded(a, b, variables, found, span, budget),
        _ if type_key_bounded(pattern, budget)? == type_key_bounded(actual, budget)? => Ok(()),
        _ => Err(error(
            span,
            "generic parameter does not match argument type",
        )),
    }
}

// Type substitutions never rewrite value names. Private const carriers replace
// only their lexically visible value uses; local patterns shadow them normally.
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
            for clause in &mut f.where_clause {
                if let BoundSubject::Type(subject) = &mut clause.subject {
                    visitor.ty(subject);
                }
                for bound in &mut clause.bounds {
                    visitor.ty(bound);
                }
            }
            let mut scoped = Cow::Borrowed(substitutions);
            for parameter in &f.params {
                if let Cow::Owned(next) = consts::scoped_name(&parameter.name, &scoped) {
                    scoped = Cow::Owned(next);
                }
            }
            let visitor = Substitution {
                substitutions: &scoped,
            };
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
        ItemKind::TypeAlias(alias) => {
            for bound in &mut alias.bounds {
                visitor.ty(bound);
            }
            if let Some(value) = &mut alias.value {
                visitor.ty(value);
            }
        }
        _ => {}
    }
    Ok(())
}
struct Substitution<'a> {
    substitutions: &'a BTreeMap<String, TypeRef>,
}
fn substitution_name<'a>(path: &'a [String]) -> Cow<'a, str> {
    if path.len() == 1 {
        Cow::Borrowed(&path[0])
    } else {
        Cow::Owned(path.join("."))
    }
}
impl Substitution<'_> {
    fn ty(&self, ty: &mut TypeRef) {
        if trait_bounds::associated::parts(ty).is_some()
            && let Ok(key) = trait_bounds::associated::equation_key_prepaid(ty)
            && let Some(value) = self.substitutions.get(&key)
        {
            let span = ty.span;
            *ty = value.clone();
            ty.span = span;
            return;
        }
        if let TypeKind::Named { path, args } = &ty.kind
            && args.is_empty()
            && let Some(value) = self.substitutions.get(substitution_name(path).as_ref())
            && consts::expression(value).is_none()
        {
            let span = ty.span;
            *ty = value.clone();
            ty.span = span;
            return;
        }
        match &mut ty.kind {
            TypeKind::Named { args, .. } | TypeKind::Dyn { args, .. } => {
                for a in args {
                    match a {
                        GenericArg::Type(t) => {
                            if let Some(name) = trait_bounds::plain(t)
                                && let Some(value) =
                                    self.substitutions.get(name).and_then(consts::expression)
                            {
                                *a = GenericArg::Const(value.clone());
                            } else {
                                self.ty(t);
                            }
                        }
                        GenericArg::Const(e) => self.expr(e),
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
                    if let Ok(value) = consts::length(e) {
                        e.kind = ExprKind::Literal(Literal::Integer(value.to_string()));
                    }
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
        let mut substitutions = Cow::Borrowed(self.substitutions);
        for s in &mut b.statements {
            Substitution {
                substitutions: &substitutions,
            }
            .statement(s);
            if let StmtKind::Binding { pattern, .. } = &s.kind {
                let scoped = consts::scoped(pattern, &substitutions);
                if let Cow::Owned(scoped) = scoped {
                    substitutions = Cow::Owned(scoped);
                }
            }
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
                if let Condition::Let { pattern, .. } = condition {
                    let scoped = consts::scoped(pattern, self.substitutions);
                    Substitution {
                        substitutions: &scoped,
                    }
                    .block(then_body);
                } else {
                    self.block(then_body);
                }
                match else_body {
                    Some(ElseBranch::Block(b)) => self.block(b),
                    Some(ElseBranch::If(s)) => self.statement(s),
                    None => {}
                }
            }
            StmtKind::While { condition, body } => {
                self.condition(condition);
                if let Condition::Let { pattern, .. } = condition {
                    let scoped = consts::scoped(pattern, self.substitutions);
                    Substitution {
                        substitutions: &scoped,
                    }
                    .block(body);
                } else {
                    self.block(body);
                }
            }
            StmtKind::For {
                pattern,
                iterable,
                body,
            } => {
                self.expr(iterable);
                let scoped = consts::scoped(pattern, self.substitutions);
                Substitution {
                    substitutions: &scoped,
                }
                .block(body);
            }
            StmtKind::Loop(b) | StmtKind::Unsafe(b) | StmtKind::Defer(b) => self.block(b),
            StmtKind::With { name, value, body } => {
                self.expr(value);
                let pattern = Pattern {
                    span: s.span,
                    kind: PatternKind::Name(name.clone()),
                };
                let scoped = consts::scoped(&pattern, self.substitutions);
                Substitution {
                    substitutions: &scoped,
                }
                .block(body);
            }
            StmtKind::Match { value, arms } => {
                self.expr(value);
                for arm in arms {
                    let scoped = consts::scoped(&arm.pattern, self.substitutions);
                    Substitution {
                        substitutions: &scoped,
                    }
                    .arrow(&mut arm.body);
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
        if self.substitutions.contains_key("Self")
            && let ExprKind::ReceiverMember(member) = &e.kind
        {
            e.kind = ExprKind::Member {
                object: Box::new(Expr {
                    kind: ExprKind::Name("self".into()),
                    span: e.span,
                }),
                member: MemberName::Name(member.clone()),
            };
        }
        if let ExprKind::Name(name) = &e.kind
            && let Some(value) = self.substitutions.get(name).and_then(consts::expression)
        {
            let span = e.span;
            *e = value.clone();
            e.span = span;
            return;
        }
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
            ExprKind::Member { object, member } => {
                self.expr(object);
                if let MemberName::Constrained { owner, .. } = member {
                    self.ty(owner);
                }
            }
            ExprKind::TypeApply { callee, arguments } => {
                self.expr(callee);
                for a in arguments {
                    match a {
                        GenericArg::Type(t) => {
                            if let Some(name) = trait_bounds::plain(t)
                                && let Some(value) =
                                    self.substitutions.get(name).and_then(consts::expression)
                            {
                                *a = GenericArg::Const(value.clone());
                            } else {
                                self.ty(t);
                            }
                        }
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
                let mut substitutions = Cow::Borrowed(self.substitutions);
                for p in params {
                    if let Some(t) = &mut p.ty {
                        self.ty(t);
                    }
                    let pattern = Pattern {
                        span: p.span,
                        kind: PatternKind::Name(p.name.clone()),
                    };
                    if let Cow::Owned(scoped) = consts::scoped(&pattern, &substitutions) {
                        substitutions = Cow::Owned(scoped);
                    }
                }
                if let Some(t) = return_type {
                    self.ty(t);
                }
                Substitution {
                    substitutions: &substitutions,
                }
                .arrow(body);
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

#[path = "generic_type_keys.rs"]
mod type_keys;
