//! Bounded, hygienic specialization of ordinary type parameters. Output is AST,
//! not a type certificate: every instantiated body enters the normal pipeline.
use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use zil_base::{CompileResult, Diagnostic, Limits, Phase, Span};
use zil_syntax::ast::*;

type R<T> = Result<T, Diagnostic>;
pub(super) fn structural_value_bound(ty: &TypeRef) -> bool {
    trait_bounds::value::is_bound(ty)
}
// The real lexical binding carries its creator-input origin. Ordinary binding
// insertion clears that origin even when the spelling and type are identical.
#[derive(Clone, Default)]
struct Env {
    bindings: BTreeMap<String, EnvBinding>,
    closure_depth: u32,
    domain: String,
}
#[derive(Clone)]
struct SemanticActual {
    application: Option<TypeRef>,
    classes: Vec<crate::stored_types::LexicalLife>,
    domain: String,
}
#[derive(Clone)]
struct EnvBinding {
    // Physical syntax is separate from the source application used by inference.
    ty: Option<TypeRef>,
    semantic: Option<SemanticActual>,
    input: Option<u32>,
    closure_depth: u32,
}
impl Env {
    fn new() -> Self {
        Self::default()
    }
    fn len(&self) -> usize {
        self.bindings.len()
    }
    fn contains_key(&self, name: &str) -> bool {
        self.bindings.contains_key(name)
    }
    fn get(&self, name: &str) -> Option<&Option<TypeRef>> {
        self.bindings.get(name).map(|binding| {
            binding
                .semantic
                .as_ref()
                .map_or(&binding.ty, |actual| &actual.application)
        })
    }
    fn values(&self) -> impl Iterator<Item = &Option<TypeRef>> {
        self.bindings.values().map(|binding| &binding.ty)
    }
    fn insert(&mut self, name: String, ty: Option<TypeRef>) -> Option<Option<TypeRef>> {
        self.bindings
            .insert(
                name,
                EnvBinding {
                    ty,
                    semantic: None,
                    input: None,
                    closure_depth: self.closure_depth,
                },
            )
            .map(|prior| prior.ty)
    }
    fn parameters(parameters: &[Param], span: Span) -> R<Self> {
        let mut env = Self::new();
        for (ordinal, parameter) in parameters.iter().enumerate() {
            let input =
                u32::try_from(ordinal).map_err(|_| error(span, "input ordinal exhausted"))?;
            env.bindings.insert(
                parameter.name.clone(),
                EnvBinding {
                    ty: Some(parameter.ty.clone()),
                    semantic: None,
                    input: Some(input),
                    closure_depth: 0,
                },
            );
        }
        Ok(env)
    }
}
#[path = "generic_budget.rs"]
mod budget;
pub(super) use budget::Budget;
#[path = "generic_alias.rs"]
mod aliases;
#[path = "generic_bytes.rs"]
mod bytes;
#[path = "generic_callable.rs"]
pub(super) mod callable;
#[path = "generic_callable_context.rs"]
pub(super) mod callable_context;
#[path = "generic_closure_context.rs"]
pub(super) mod closure_context;
#[path = "generic_constant_plan.rs"]
pub(crate) mod constant_plan;
#[path = "generic_const.rs"]
mod consts;
#[path = "generic_copy.rs"]
mod copy;
#[path = "generic_enum.rs"]
mod enums;
#[path = "generic_helper_graph.rs"]
mod helper_graph;
#[path = "generic_inherent.rs"]
mod inherent;
#[path = "generic_options.rs"]
pub(super) mod options;
#[path = "generic_origins.rs"]
pub(super) mod origins;
#[path = "generic_products.rs"]
pub(super) mod products;
#[path = "generic_record_enum.rs"]
pub(super) mod record_enums;
#[path = "generic_rich_pointer.rs"]
pub(super) mod rich_pointer;
#[path = "generic_runtime_ct.rs"]
mod runtime_ct_phase;
#[path = "generic_source_life.rs"]
mod source_life;
#[path = "generic_state.rs"]
mod states;
#[path = "generic_store_templates.rs"]
pub(super) mod store_templates;
#[path = "generic_stored_actuals.rs"]
mod stored_actuals;
use stored_actuals::source_unification_key;
#[path = "generic_stored_nominal.rs"]
mod stored_nominal;
#[path = "generic_store.rs"]
mod stores;
#[path = "generic_trait_bounds.rs"]
mod trait_bounds;
pub(super) use trait_bounds::dynamic;
#[path = "generic_trait_static_host.rs"]
mod static_host;
#[path = "generic_trait_defaults.rs"]
pub(super) mod trait_defaults;
#[path = "generic_trait_impls.rs"]
mod trait_impls;
#[path = "generic_trait_instances.rs"]
pub(super) mod trait_instances;
#[path = "generic_trait_methods.rs"]
mod trait_methods;
#[path = "generic_where.rs"]
mod where_copy;

fn check_nominal_name(name: &str, span: Span) -> R<()> {
    if name == "Option"
        || super::byte_buffer_name(name)
        || super::owned_store_name(name)
        || super::registration_name(name)
    {
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
    specialize_products_with_context(module, limits, products, None)
}
pub(super) fn specialize_products_with_context<'a>(
    module: &'a Module,
    limits: &Limits,
    products: &mut products::Products,
    context: Option<(
        &[zil_base::SourceUnit],
        super::source_context::SourceIdentityContext<'_>,
    )>,
) -> CompileResult<Cow<'a, Module>> {
    specialize_products_with_runtime(module, limits, products, context, None)
}

pub(super) fn specialize_products_with_runtime<'a>(
    module: &'a Module,
    limits: &Limits,
    products: &mut products::Products,
    context: Option<(
        &[zil_base::SourceUnit],
        super::source_context::SourceIdentityContext<'_>,
    )>,
    runtime_ct: Option<&dyn crate::runtime_ct::RuntimeCt>,
) -> CompileResult<Cow<'a, Module>> {
    let (prepared, remaining) = options::prepare(module, limits)?;
    match prepared {
        Some(prepared) => specialize_products_prepared(
            &prepared.module,
            &remaining,
            products,
            prepared.prelude,
            context,
            runtime_ct,
        )
        .map(|module| Cow::Owned(module.into_owned())),
        None => specialize_products_prepared(
            module,
            &remaining,
            products,
            origins::CompilerPrelude::default(),
            context,
            runtime_ct,
        ),
    }
}

fn specialize_products_prepared<'a>(
    module: &'a Module,
    limits: &Limits,
    products: &mut products::Products,
    prelude: origins::CompilerPrelude,
    context: Option<(
        &[zil_base::SourceUnit],
        super::source_context::SourceIdentityContext<'_>,
    )>,
    runtime_ct: Option<&dyn crate::runtime_ct::RuntimeCt>,
) -> CompileResult<Cow<'a, Module>> {
    let mut names_budget = Budget::new(limits);
    if runtime_ct.is_some() {
        names_budget.note_source_identity();
    }
    names_budget.target_pointer_width = products.target_pointer_width.unwrap_or(64);
    super::lifetimes::validate(module, &mut names_budget)?;
    let mut has_where = false;
    let mut has_defaults = false;
    let mut has_associated = false;
    let mut has_generic_methods = false;
    let mut has_impl_recipes = false;
    let mut has_inherent = false;
    let mut has_alias = false;
    for item in &module.items {
        names_budget.node(item.span, 0).map_err(|e| vec![e])?;
        has_where |= matches!(&item.kind, ItemKind::Function(f) if !f.where_clause.is_empty());
        has_alias |= matches!(item.kind, ItemKind::TypeAlias(_));
        has_impl_recipes |= matches!(&item.kind, ItemKind::Impl(i) if i.trait_type.is_some() && (!i.generics.is_empty() || !i.where_clause.is_empty() || trait_bounds::plain(&i.target).is_none() || trait_instances::head(&i.target).is_some_and(super::static_traits::scalar_self)));
        if matches!(&item.kind, ItemKind::Impl(i) if i.trait_type.is_none() && !i.generics.is_empty())
        {
            has_impl_recipes = true;
            has_inherent = true;
            names_budget.note_source_identity();
        }
        if let ItemKind::Trait(declaration) = &item.kind {
            let (defaults, associated, methods) =
                trait_defaults::features_with_methods(declaration, &mut names_budget)
                    .map_err(|e| vec![e])?;
            has_generic_methods |= methods;
            has_associated |= associated;
            has_defaults |= defaults || associated;
            has_defaults |= !declaration.bounds.is_empty();
        }
        if let ItemKind::Impl(implementation) = &item.kind
            && implementation.trait_type.as_ref().is_some_and(|bound| {
                stores::is_bound(bound)
                    || trait_bounds::value::is_bound(bound)
                    || callable::closed_bound(bound)
            })
        {
            return Err(vec![error(
                item.span,
                "compiler structural bounds cannot be implemented",
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
                if matches!(item.kind, ItemKind::Aggregate(_) | ItemKind::Enum(_)) {
                    names_budget.note_source_identity();
                }
                continue;
            }
            names_budget.node(param.span, 0).map_err(|e| vec![e])?;
            if super::byte_buffer_name(&param.name)
                || super::owned_store_name(&param.name)
                || super::registration_name(&param.name)
                || param.name == "Option"
            {
                return Err(vec![super::primitive_name_error(&param.name, param.span)]);
            }
        }
    }
    if has_generic_methods || names_budget.needs_source_identity() {
        if let Some(origins) = &mut products.origins {
            if prelude.len != 0 {
                origins
                    .prepend_compiler_prelude(module, prelude, &mut names_budget)
                    .map_err(|e| vec![e])?;
            }
        } else {
            products.origins = Some(
                origins::Origins::single_prepared(module, prelude, &mut names_budget)
                    .map_err(|e| vec![e])?,
            );
        }
    }
    if names_budget.needs_source_identity()
        && let Some((sources, context)) = context
    {
        let resolved = context
            .resolve(sources, &mut names_budget)
            .map_err(|e| vec![e])?;
        products
            .origins
            .as_mut()
            .expect("dyn source ledger")
            .capture_prepared_identities(module, prelude, &resolved, &mut names_budget)
            .map_err(|e| vec![e])?;
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
            (has_associated, has_generic_methods, has_inherent),
            runtime_ct,
        )
        .map(Cow::Owned)
        .map_err(|e| vec![e]);
    }
    if has_where {
        let normalized = where_copy::normalize(module, &mut names_budget).map_err(|e| vec![e])?;
        let (aliased, budget) =
            aliases::normalize_with_origins(&normalized, names_budget, products.origins.as_mut())
                .map_err(|e| vec![e])?;
        return match aliased {
            Some(mut aliased) => {
                products.rich_aliases.append(&mut aliased.rich_aliases);
                specialize_with_budget(
                    &aliased.module,
                    limits,
                    &aliased.pattern_types,
                    budget,
                    products,
                    (has_associated, has_generic_methods, has_inherent),
                    runtime_ct,
                )
            }
            None => specialize_with_budget(
                &normalized,
                limits,
                &BTreeMap::new(),
                budget,
                products,
                (has_associated, has_generic_methods, has_inherent),
                runtime_ct,
            ),
        }
        .map(Cow::Owned)
        .map_err(|e| vec![e]);
    }
    let (normalized, names_budget) =
        aliases::normalize_with_origins(module, names_budget, products.origins.as_mut())
            .map_err(|e| vec![e])?;
    if let Some(mut normalized) = normalized {
        products.rich_aliases.append(&mut normalized.rich_aliases);
        if normalized.pattern_types.is_empty()
            && normalized
                .module
                .items
                .iter()
                .all(|item| params(item).is_empty())
            && !has_defaults
            && !has_impl_recipes
            && !has_generic_methods
            && !normalized
                .module
                .items
                .iter()
                .any(|item| matches!(&item.kind,ItemKind::Trait(t) if !t.generics.is_empty()))
            && !names_budget.has_enum_profiles()
            && !names_budget.needs_source_identity()
            && !names_budget.has_borrowed_keys()
        {
            return Ok(Cow::Owned(normalized.module));
        }
        return specialize_with_budget(
            &normalized.module,
            limits,
            &normalized.pattern_types,
            names_budget,
            products,
            (has_associated, has_generic_methods, has_inherent),
            runtime_ct,
        )
        .map(Cow::Owned)
        .map_err(|error| vec![error]);
    }
    if module.items.iter().all(|item| params(item).is_empty())
        && !has_defaults
        && !has_impl_recipes
        && !has_generic_methods
        && !module
            .items
            .iter()
            .any(|item| matches!(&item.kind,ItemKind::Trait(t) if !t.generics.is_empty()))
        && !names_budget.has_enum_profiles()
        && !names_budget.needs_source_identity()
        && !names_budget.has_borrowed_keys()
    {
        return Ok(Cow::Borrowed(module));
    }
    specialize_with_budget(
        module,
        limits,
        &BTreeMap::new(),
        names_budget,
        products,
        (has_associated, has_generic_methods, has_inherent),
        runtime_ct,
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
    type_features: (bool, bool, bool),
    runtime_ct: Option<&'a dyn crate::runtime_ct::RuntimeCt>,
) -> R<Module> {
    let (has_associated, has_generic_methods, has_inherent) = type_features;
    let mut origins = products.origins.take();
    let inherent = inherent::prepare(module, has_inherent, origins.as_mut(), &mut budget)?;
    let module = inherent
        .as_ref()
        .map_or(module, |prepared| &prepared.module);
    let inherent_methods = inherent
        .as_ref()
        .map_or(&[][..], |prepared| prepared.methods.as_slice());
    source_life::validate(module, &mut budget)?;
    let methods =
        trait_methods::prepare(module, has_generic_methods, origins.as_ref(), &mut budget)?;
    let module = methods.as_ref().map_or(module, |prepared| &prepared.module);
    let prepared = trait_bounds::associated::prepare(
        module,
        has_associated,
        methods.as_ref().map(|prepared| &prepared.bindings),
        origins.as_ref(),
        &mut budget,
    )?;
    let module = prepared.as_ref().unwrap_or(module);
    let lifted = trait_methods::lift(module, has_generic_methods, origins.as_mut(), &mut budget)?;
    let module = lifted.as_ref().unwrap_or(module);
    let mut engine = Engine::with_budget(
        module,
        limits,
        pattern_types,
        origins.as_ref(),
        inherent_methods,
        budget,
        runtime_ct,
    )?;
    engine.runtime_ct = runtime_ct;
    if !engine.nominal_binder_collision {
        return engine.run(module, products);
    }
    if !inherent_methods.is_empty() {
        return Err(unsupported(
            module.span,
            "generic inherent binder collides with a nominal declaration",
        ));
    }
    // Generated aggregates no longer carry the original generic declaration.
    // Protect actual nominal identities in the original, still-scoped AST,
    // before substituting a caller binder into those aggregate fields.
    let protected = super::modules::protect_alias_nominals(module, &mut engine.budget)?;
    let mut engine = Engine::with_budget(
        &protected,
        limits,
        pattern_types,
        origins.as_ref(),
        inherent_methods,
        engine.budget,
        runtime_ct,
    )?;
    engine.runtime_ct = runtime_ct;
    if engine.nominal_binder_collision {
        return Err(error(
            module.span,
            "type binder still collides with a nominal identity after scoped normalization",
        ));
    }
    engine.run(&protected, products)
}

type StoredEnumLayout = (String, Vec<(String, Vec<TypeRef>)>);

struct Engine<'a> {
    inherent_methods: &'a [inherent::Method],
    inherent_instances: BTreeMap<String, (String, BTreeMap<String, TypeRef>)>,
    runtime_ct: Option<&'a dyn crate::runtime_ct::RuntimeCt>,
    runtime_calls: BTreeMap<usize, u32>,
    runtime_call: Option<u32>,
    runtime_instance: Option<u32>,
    runtime_next_instance: u32,
    runtime_instances: BTreeMap<String, u32>,
    original_module: &'a Module,
    closure_recording: Option<closure_context::Recording>,
    life_scope: Vec<String>,
    nominal_life_bindings: Vec<(String, crate::stored_types::LexicalLife)>,
    nominal_captured_lives: Vec<(String, crate::stored_types::LexicalLife)>,
    next_nominal_capture: usize,
    nominal_source_actuals: BTreeMap<String, TypeRef>,
    // Concrete source field layouts for stored physical records. Hints only;
    // storage classes and actual permissions live in the independent catalog.
    stored_record_fields: BTreeMap<String, Vec<(String, TypeRef)>>,
    stored_source_parameters: BTreeMap<String, Vec<Option<SemanticActual>>>,
    stored_enum_layouts: BTreeMap<String, StoredEnumLayout>,
    stored_nominals: bool,
    pattern_types: &'a BTreeMap<String, TypeRef>,
    templates: BTreeMap<String, &'a Item>,
    enums: BTreeMap<String, &'a EnumDecl>,
    nominals: BTreeMap<String, &'a Item>,
    traits: trait_bounds::Index<'a>,
    trait_calls: trait_instances::Calls,
    selected_error_arms: BTreeMap<String, error_injection::Choices>,
    resolved_method_calls: trait_instances::Calls,
    method_calls: trait_instances::CallTargets,
    return_type: Option<TypeRef>,
    // The original full enum result application, captured before a stored
    // signature replaces it with its physical name. Only body checking reads
    // this hint; the emitted function header remains physical.
    source_enum_return: Option<TypeRef>,
    source_enum_return_active: bool,
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
    impl_error_arms: BTreeMap<(usize, usize), error_injection::Choices>,
    default_error_arms: BTreeMap<(String, String), error_injection::Choices>,
    impl_calls: BTreeMap<(usize, usize), trait_instances::CallTargets>,
    instance_types: BTreeMap<String, TypeRef>,
    anonymous_function: Option<String>,
    anonymous_substitutions: BTreeMap<String, BTreeMap<String, TypeRef>>,
    state_families: BTreeMap<(String, Vec<Option<String>>), usize>,
    pending: VecDeque<origins::Pending>,
    origins: Option<&'a origins::Origins>,
    current_origin: Option<origins::Origin>,
    current_member_calls: BTreeMap<usize, trait_instances::CallTargets>,
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
        origins: Option<&'a origins::Origins>,
        inherent_methods: &'a [inherent::Method],
        mut budget: Budget<'a>,
        runtime_ct: Option<&'a dyn crate::runtime_ct::RuntimeCt>,
    ) -> R<Self> {
        let traits = trait_bounds::Index::with_origins(module, origins, &mut budget)?;
        let mut engine = Self {
            inherent_methods,
            inherent_instances: BTreeMap::new(),
            runtime_ct,
            runtime_calls: BTreeMap::new(),
            runtime_call: None,
            runtime_instance: None,
            runtime_next_instance: 0,
            runtime_instances: BTreeMap::new(),
            original_module: module,
            closure_recording: None,
            life_scope: Vec::new(),
            nominal_life_bindings: Vec::new(),
            nominal_captured_lives: Vec::new(),
            next_nominal_capture: 0,
            nominal_source_actuals: BTreeMap::new(),
            stored_record_fields: BTreeMap::new(),
            stored_source_parameters: BTreeMap::new(),
            stored_enum_layouts: BTreeMap::new(),
            stored_nominals: budget.has_stored_lifetimes(),
            pattern_types,
            traits,
            trait_calls: trait_instances::Calls::new(),
            selected_error_arms: BTreeMap::new(),
            resolved_method_calls: trait_instances::Calls::new(),
            method_calls: trait_instances::CallTargets::new(),
            templates: BTreeMap::new(),
            enums: BTreeMap::new(),
            nominals: BTreeMap::new(),
            return_type: None,
            source_enum_return: None,
            source_enum_return_active: false,
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
            impl_error_arms: BTreeMap::new(),
            default_error_arms: BTreeMap::new(),
            impl_calls: BTreeMap::new(),
            instance_types: BTreeMap::new(),
            anonymous_function: None,
            anonymous_substitutions: BTreeMap::new(),
            state_families: BTreeMap::new(),
            pending: VecDeque::new(),
            origins,
            current_origin: None,
            current_member_calls: BTreeMap::new(),
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
            let lifted_life = if let ItemKind::Function(f) = &item.kind
                && super::lifetimes::only_life(f)
                && f.name.as_bytes().first() == Some(&b'$')
            {
                engine.budget.text(&f.name, f.span)?;
                f.name.starts_with("$generic_method_")
                    && engine
                        .traits
                        .origin(item, &mut engine.budget)?
                        .is_some_and(|origin| origin.provider)
            } else {
                false
            };
            if !params(item).is_empty() || lifted_life {
                let key = name(item).expect("template has a name").to_owned();
                if engine.templates.insert(key, item).is_some() {
                    return Err(error(item.span, "duplicate generic declaration"));
                }
            }
        }
        engine.validate_impl_recipes()?;
        engine.validate_trait_defaults(module)?;
        engine.validate_trait_constants(module)?;
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
            engine.budget.context.origin = engine.traits.origin(item, &mut engine.budget)?;
            if !item.attributes.is_empty() {
                return Err(unsupported(
                    item.span,
                    "unexecuted generic declaration attribute",
                ));
            }
            let callable = if let ItemKind::Function(function) = &item.kind {
                callable::template(function, &mut engine.budget)?
            } else {
                None
            };
            // The parameter loop already visits every formal. Only an actual
            // lifetime formal needs this extra profile query; callable
            // classification above has already established its own profile.
            let mut allow_source_life = None;
            let mut seen = BTreeSet::new();
            for parameter in params(item) {
                if matches!(parameter.kind, GenericParamKind::Lifetime)
                    && let ItemKind::Function(f) = &item.kind
                {
                    let allowed = match allow_source_life {
                        Some(allowed) => allowed,
                        None => {
                            let allowed = if callable.is_some() {
                                true
                            } else {
                                engine.budget.identity_work(f.generics.len(), f.span)?;
                                if super::lifetimes::copy_life(f)
                                    || super::lifetimes::store_element_life(f)
                                    || super::lifetimes::nominal_life(f, &mut engine.budget)?
                                {
                                    true
                                } else if engine
                                    .budget
                                    .context
                                    .origin
                                    .is_some_and(|origin| origin.provider)
                                {
                                    engine.budget.text(&f.name, f.span)?;
                                    f.name.starts_with("$generic_method_")
                                        && super::lifetimes::only_life(f)
                                } else {
                                    false
                                }
                            };
                            allow_source_life = Some(allowed);
                            allowed
                        }
                    };
                    if allowed {
                        continue;
                    }
                }
                if matches!(parameter.kind, GenericParamKind::Lifetime)
                    && matches!(item.kind, ItemKind::Aggregate(_) | ItemKind::Enum(_))
                {
                    continue;
                }
                // This parameter/name was already charged by item preflight.
                // Borrow the existing nominal index; do not add a global scan,
                // name copy or a hygiene pass to the noncolliding old path.
                if matches!(parameter.kind, GenericParamKind::Type(_))
                    && matches!(parameter.name.as_str(), "Value" | "SyncDrop")
                {
                    return Err(error(
                        parameter.span,
                        "Value and SyncDrop are reserved in type-binder and bound positions",
                    ));
                }
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
                            || matches!(&item.kind, ItemKind::Aggregate(a)
                                if matches!(a.kind, AggregateKind::Struct | AggregateKind::Package))
                                && bounds.iter().all(|bound| {
                                    copy::is_bound(bound)
                                        || stores::is_bound(bound)
                                        || trait_instances::head(bound).is_some()
                                })
                            || matches!(item.kind, ItemKind::Enum(_))
                                && bounds.iter().all(|bound| {
                                    copy::is_bound(bound) || trait_instances::head(bound).is_some()
                                })
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
                if !f.where_clause.is_empty()
                    && !f.where_clause.iter().all(|clause| {
                        clause.lifetime_bound.is_none()
                            && matches!(&clause.subject, BoundSubject::Type(subject)
                                if trait_bounds::associated::parts(subject).is_some())
                            && !clause.bounds.is_empty()
                            && clause.bounds.iter().all(|bound| {
                                copy::is_bound(bound)
                                    || stores::is_bound(bound)
                                    || trait_bounds::value::is_bound(bound)
                            })
                    })
                    && !engine
                        .budget
                        .context
                        .origin
                        .is_some_and(|origin| origin.provider)
                {
                    return Err(unsupported(f.span, "generic where/contract solving"));
                }
                stores::result_mode(f)?;
                let ct_types = runtime_ct_phase::symbolic_types(
                    item,
                    engine.budget.context.origin,
                    engine.origins,
                    module,
                    engine.runtime_ct,
                    &mut engine.budget,
                )?;
                let source_ids = if ct_types.is_some() {
                    Some(trait_instances::expression_ids(
                        &f.body,
                        &mut engine.budget,
                    )?)
                } else {
                    None
                };
                let checked = consts::definition(
                    f,
                    &mut engine.budget,
                    engine.traits.has_instances || ct_types.is_some(),
                    source_ids,
                )?;
                let selected = validate_parametric(
                    &checked,
                    check::ParametricTables {
                        ct_types: ct_types.as_ref(),
                        source_definition: Some(f),
                        expression_origins: checked.source_ids.as_ref(),
                        definition_equations: None,
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
                if !selected.is_empty() {
                    engine.budget.text(&f.name, f.span)?;
                    engine.budget.lineage_bytes(
                        std::mem::size_of::<(String, error_injection::Choices)>(),
                        f.span,
                    )?;
                    engine.selected_error_arms.insert(f.name.clone(), selected);
                }
            }
            if let ItemKind::Enum(e) = &item.kind
                && !engine.budget.enum_profile(item.span)
                && e.name != options::TEMPLATE
                && e.name != options::TASK_POLL
                && e.name != options::TASK_THREAD_SCOPE
            {
                enums::validate_declaration(
                    e,
                    item.span,
                    &engine.nominals,
                    &engine.traits,
                    &mut engine.budget,
                )?;
            }
            if let ItemKind::Aggregate(a) = &item.kind {
                let variables: BTreeSet<_> = a
                    .generics
                    .iter()
                    .filter(|p| !matches!(p.kind, GenericParamKind::Lifetime))
                    .map(|p| p.name.clone())
                    .collect();
                let bounds = engine
                    .traits
                    .parameter_bounds(&a.generics, &mut engine.budget)?;
                // Ordinary nominal bounds prove members, not a field's Copy
                // layout or its owning cleanup. This finite record profile
                // needs that layout proof even when no instance is requested.
                let nominal_fields = bounds
                    .values()
                    .flatten()
                    .any(|bound| matches!(bound, trait_bounds::Bound::Nominal(_)));
                if a.kind == AggregateKind::Struct && nominal_fields {
                    for field in &a.fields {
                        if let Err(diagnostic) = copy::require(
                            &field.ty,
                            &engine.nominals,
                            &bounds,
                            &variables,
                            &mut engine.budget,
                        ) {
                            if diagnostic.category == zil_base::ResultCategory::ResourceLimit {
                                return Err(diagnostic);
                            }
                            return Err(unsupported(
                                field.span,
                                "generic nominal-bound record needs a complete Copy field layout; general owning fields require their Value cleanup profile",
                            ));
                        }
                    }
                }
                // A Copy-only Package has no owning leaf in the current
                // Product profile. An unused template cannot erase that
                // definition requirement before Types sees a declaration.
                let constrained_package = a.kind == AggregateKind::Package
                    && bounds.values().flatten().any(|bound| {
                        matches!(
                            bound,
                            trait_bounds::Bound::Copy | trait_bounds::Bound::Nominal(_)
                        )
                    });
                if constrained_package && !a.fields.iter().any(|field| field.manages) {
                    let mut copy_only = true;
                    for field in &a.fields {
                        match copy::require(
                            &field.ty,
                            &engine.nominals,
                            &bounds,
                            &variables,
                            &mut engine.budget,
                        ) {
                            Ok(()) => {}
                            Err(diagnostic)
                                if diagnostic.category
                                    == zil_base::ResultCategory::ResourceLimit =>
                            {
                                return Err(diagnostic);
                            }
                            Err(_) => copy_only = false,
                        }
                    }
                    if copy_only {
                        return Err(unsupported(
                            item.span,
                            "generic Package requires a manages field or complete owning child in the current Product profile",
                        ));
                    }
                }
                for field in &a.fields {
                    if a.kind == AggregateKind::Package
                        && !field.manages
                        && bounds.values().flatten().any(|bound| {
                            matches!(
                                bound,
                                trait_bounds::Bound::Value
                                    | trait_bounds::Bound::SyncDrop
                                    | trait_bounds::Bound::Nominal(_)
                            )
                        })
                    {
                        engine.traits.value_bound(
                            &field.ty,
                            &trait_bounds::Bound::Value,
                            &bounds,
                            &variables,
                            &mut engine.budget,
                        )?;
                    }
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
                if !matches!(a.kind, AggregateKind::Struct | AggregateKind::Package) {
                    return Err(unsupported(
                        item.span,
                        "generic resource or methods require capability resolution",
                    ));
                }
                if !a.methods.is_empty() {
                    runtime_ct_phase::check_static_host(
                        item,
                        engine.budget.context.origin,
                        engine.origins,
                        engine.runtime_ct,
                        &mut engine.budget,
                    )?;
                }
            }
        }
        helper_dependencies.check(&mut engine.budget)?;
        if engine.budget.has_dynamic() {
            engine.products.dynamic = dynamic::collect(
                module,
                &engine.traits,
                origins.ok_or_else(|| error(module.span, "dyn source provenance absent"))?,
                &mut engine.budget,
            )?;
        }
        Ok(engine)
    }
    fn tick(&mut self, span: Span, depth: usize) -> R<()> {
        self.budget.node(span, depth)
    }
    fn run(&mut self, module: &Module, products: &mut products::Products) -> R<Module> {
        self.require_evaluated_constants(module)?;
        self.products.trait_instances = self.traits.has_instances;
        self.products.record_variants = self.budget.has_enum_profiles();
        self.products.protected_binders = self.budget.has_primitive_binders();
        self.products.borrowed_signatures = self.budget.has_borrowed_keys();
        self.products.storage.borrowed_signatures = self.products.borrowed_signatures;
        self.collect_products(module)?;
        // Aliases are checked even when they have no runtime use. These are
        // complete paid originals retained by alias normalization, not bodies
        // recovered from a convenient concrete instantiation.
        for ty in std::mem::take(&mut products.rich_aliases) {
            self.prepare_rich_pointer(&ty, 0)?;
        }
        let mut result = Module {
            id: module.id,
            span: module.span,
            items: vec![],
        };
        for (ordinal, item) in module.items.iter().enumerate() {
            if matches!(&item.kind, ItemKind::Impl(i) if i.trait_type.is_some() && (!i.generics.is_empty() || !i.where_clause.is_empty() || trait_bounds::plain(&i.target).is_none()))
            {
                continue;
            }
            if params(item).is_empty()
                || matches!(&item.kind, ItemKind::Function(f)
                    if callable::template(f, &mut self.budget)?.is_some_and(|bounds| f.generics.iter().all(|p| matches!(p.kind,GenericParamKind::Lifetime) || bounds.contains_generic(&p.name))))
            {
                self.budget.item(item)?;
                self.enqueue(
                    item.clone(),
                    self.origins
                        .map(|origins| origins.get(ordinal, item.span))
                        .transpose()?,
                )?;
                if self.traits.has_generic_methods && matches!(item.kind, ItemKind::Impl(_)) {
                    let calls = self.impl_method_calls(item, &BTreeMap::new())?;
                    self.pending.back_mut().expect("just enqueued").member_calls = calls;
                }
            }
        }
        self.prepare_stored_headers()?;
        let mut output_origins = origins::Origins::default();
        while let Some(origins::Pending {
            mut item,
            origin,
            source_enum_return,
            member_calls,
            runtime_calls,
        }) = self.pending.pop_front()
        {
            // The source result belongs to this exact pending item, even if
            // header preparation enqueued other specializations first.
            self.source_enum_return = source_enum_return;
            self.runtime_instance = name(&item)
                .and_then(|name| self.runtime_instances.get(name))
                .copied();
            self.runtime_calls = match runtime_calls {
                Some(anchors) => anchors,
                None => self.runtime_call_anchors(&item)?,
            };
            // Concrete specializations inherit the checked provider origin;
            // their exact generated spelling has one additional prefix.
            let source_life_provider = origin.is_some_and(|origin| origin.provider)
                && matches!(&item.kind, ItemKind::Function(function)
                    if (function.name.starts_with("$generic_method_")
                        || function.name.starts_with("$generic_$generic_method_"))
                        && super::lifetimes::only_life(function));
            let previous = std::mem::replace(&mut self.current_member_calls, member_calls);
            let processed = self.with_origin(origin, |engine| engine.item(&mut item));
            self.current_member_calls = previous;
            processed?;
            if source_life_provider {
                let ItemKind::Function(function) = &item.kind else {
                    return Err(error(
                        item.span,
                        "method provider lost its function declaration",
                    ));
                };
                // Stored Option headers erase only their source life formals
                // after recording the complete relation in the catalog.
                if function.generics.is_empty() {
                    self.stored_lookup_fee(
                        self.products.storage.functions.len(),
                        function.name.len(),
                        item.span,
                    )?;
                    if !self.products.storage.functions.contains_key(&function.name) {
                        return Err(error(item.span, "method provider lost its life relation"));
                    }
                }
                self.budget.text(&function.name, item.span)?;
                self.budget.identity_work(
                    self.products
                        .method_providers
                        .len()
                        .checked_ilog2()
                        .map_or(1, |n| n as usize + 2),
                    item.span,
                )?;
                if !self.products.method_providers.insert(function.name.clone()) {
                    return Err(error(item.span, "duplicate method provider identity"));
                }
            }
            if self.origins.is_some() {
                self.budget.node(item.span, 0)?;
                output_origins
                    .entries
                    .push(origin.ok_or_else(|| error(item.span, "output provenance absent"))?);
            }
            result.items.push(item);
        }
        if let Some(original) = self.origins {
            output_origins.identities = original.copy_identities(&mut self.budget, module.span)?;
            self.products.origins = Some(output_origins);
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
        if template == options::TASK_THREAD_SCOPE {
            for actual in &arguments {self.budget.ty(actual)?;}
            if !matches!(arguments.as_slice(),[actual] if options::thread_owner_syntax(actual)) {
                return Err(error(span,"TaskThreadScopeResult requires one whole Managed<Task> owner"));
            }
        }
        let item = self
            .templates
            .get(template)
            .cloned()
            .ok_or_else(|| error(span, format!("unknown generic declaration {template}")))?;
        if matches!(&item.kind, ItemKind::Aggregate(a) if !a.methods.is_empty()) {
            return Err(unsupported(
                span,
                "generic resource or methods require capability resolution",
            ));
        }
        let product =
            matches!(&item.kind, ItemKind::Aggregate(a) if products::parameters(a).is_some());
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
            ItemKind::Function(function) => callable::template(function, &mut self.budget)?,
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
        let mut arguments = parameters
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
        // The source application still names its complete nominal arguments.
        // Resolve a qualified Type actual while the enclosing function's
        // lexical life scope is available, before the ordinary function key
        // and substitution erase that source relation. CT receives the
        // original actual separately and authenticates it from the Call.
        let mut source_arguments = None;
        let mut class_keys: Option<Vec<Option<String>>> = None;
        if self.stored_nominals && !self.life_scope.is_empty() && template != options::TASK_THREAD_SCOPE {
            let mut qualified = false;
            for actual in &arguments {
                if self.qualified_actual_syntax(actual, 0)? {
                    qualified = true;
                }
            }
            if qualified {
                for actual in &arguments {
                    self.budget.ty(actual)?;
                    self.budget
                        .lineage_bytes(std::mem::size_of::<Option<String>>(), span)?;
                }
                source_arguments = Some(arguments.clone());
                let mut keys = vec![None; arguments.len()];
                let mut joint_classes = Vec::new();
                for (ordinal, actual) in arguments.iter_mut().enumerate() {
                    if let Some(stored) = self.qualified_type_actual(actual, 0)? {
                        keys[ordinal] = Some(self.joint_class_key(
                            &stored.classes,
                            &mut joint_classes,
                            span,
                        )?);
                    }
                }
                class_keys = Some(keys);
            }
        }
        let mut key = (
            template.to_owned(),
            arguments
                .iter()
                .enumerate()
                .map(|(ordinal, ty)| {
                    let physical = type_key(ty, &mut self.budget)?;
                    if let Some(classes) =
                        class_keys.as_ref().and_then(|keys| keys[ordinal].as_ref())
                    {
                        self.budget
                            .identity_work(physical.len().saturating_add(classes.len()), span)?;
                        Ok(format!("{physical}[{classes}]"))
                    } else {
                        Ok(physical)
                    }
                })
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
        let runtime_substitution = if let Some(source) = source_arguments {
            for parameter in &parameters {
                self.budget.text(&parameter.name, parameter.span)?;
                self.budget
                    .lineage_bytes(std::mem::size_of::<(String, TypeRef)>(), span)?;
            }
            Some(
                parameters
                    .iter()
                    .zip(source)
                    .map(|(p, ty)| (p.name.clone(), ty))
                    .collect::<BTreeMap<_, _>>(),
            )
        } else {
            None
        };
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
        let origin = self.template_origin(item)?;
        self.with_origin(origin, |engine| {
            engine.traits.check_arguments(
                params(item),
                &substitution,
                (
                    &product_bounds,
                    &product_variables,
                    callable::StoreCallbacks::from_names(
                        engine.callable_name.as_deref(),
                        engine.callable_pair.as_ref(),
                    ),
                ),
                &mut engine.budget,
                span,
                retained.as_ref(),
            )?;
            if let ItemKind::Function(function) = &item.kind {
                engine.traits.check_where(
                    &function.where_clause,
                    &substitution,
                    &product_bounds,
                    &product_variables,
                    &mut engine.budget,
                )?;
            }
            engine.realize_bounds(params(item), &substitution, span, retained.as_ref())?;
            Ok(())
        })?;
        self.budget.substituted_item(item, &substitution)?;
        consts::prepay_scopes(item, &substitution, &mut self.budget)?;
        let mut instance = item.clone();
        // Anchor actual owned callees before substitutions or CT replacement
        // can change surrounding syntax. Pending processing consumes these
        // anchors directly; it never recounts calls in materialized output.
        let runtime_calls = self.runtime_call_anchors(&instance)?;
        let error_arguments = if let ItemKind::Function(function) = &instance.kind
            && let Some(selected) = self.selected_error_arms.get(&function.name)
        {
            error_injection::anchor_arguments(function, selected, &mut self.budget)?
        } else {
            error_injection::ArgumentAnchors::default()
        };
        let mut anchors = None;
        let static_direct = if let ItemKind::Function(function) = &item.kind {
            self.trait_calls.get(&function.name).is_some_and(|calls| {
                calls
                    .values()
                    .any(|target| target.static_host && !target.generic)
            })
        } else {
            false
        };
        let mut materialized_hosts = BTreeMap::new();
        if static_direct {
            // A type binder's nominal actual may itself need specialization
            // before the ordinary static impl gets its emitted function name.
            // Only origin-backed static call records enter this path.
            for (name, actual) in &substitution {
                if !matches!(&grouped(actual).kind, TypeKind::Named { path, .. } if path.len() == 1)
                {
                    continue;
                }
                self.budget.text(name, span)?;
                self.budget.ty(actual)?;
                self.budget
                    .lineage_bytes(std::mem::size_of::<(String, TypeRef)>(), span)?;
                let mut actual = actual.clone();
                self.ty(&mut actual, 0)?;
                materialized_hosts.insert(name.clone(), actual);
            }
        }
        if let ItemKind::Function(function) = &item.kind
            && let Some(calls) = self.trait_calls.get(&function.name)
        {
            let normalized =
                self.traits
                    .normalize_call_targets(calls, &substitution, &mut self.budget, span)?;
            let calls = normalized.as_ref();
            if self.traits.has_generic_methods {
                let resolved = trait_instances::resolve_generic_calls(
                    calls,
                    &substitution,
                    &mut self.budget,
                    span,
                )?;
                anchors = Some(trait_instances::anchor_resolved_calls(
                    &instance,
                    resolved,
                    &mut self.budget,
                )?);
            }
            if static_direct {
                trait_instances::rewrite_calls_materialized(
                    &mut instance,
                    calls,
                    &substitution,
                    &materialized_hosts,
                    &mut self.budget,
                )?;
            } else {
                trait_instances::rewrite_calls(
                    &mut instance,
                    calls,
                    &substitution,
                    &mut self.budget,
                )?;
            }
        }
        if let ItemKind::Function(function) = &mut instance.kind
            && let Some(selected) = self.selected_error_arms.get(&function.name)
        {
            let moved = error_injection::apply_with_arguments(
                function,
                selected,
                error_arguments,
                &mut self.budget,
            )?;
            if let Some(anchors) = anchors.as_mut() {
                anchors.relocate(&moved, &mut self.budget, span)?;
            }
        }
        if let Some(source) = &runtime_substitution {
            let (source, binders) = self.alpha_value_actuals(source, &parameters, item, span)?;
            self.budget.substituted_item(item, &source)?;
            consts::prepay_scopes(item, &source, &mut self.budget)?;
            substitute_item(&mut instance, &source)?;
            if let ItemKind::Function(function) = &mut instance.kind {
                function.generics.extend(binders);
            }
        } else {
            substitute_item(&mut instance, &substitution)?;
        }
        self.runtime_materialize(
            item,
            runtime_substitution.as_ref().unwrap_or(&substitution),
            &generated,
            &mut instance,
        )?;
        if let Some(anchors) = anchors {
            let calls =
                trait_instances::rekey_anchored_calls(&instance, anchors, &mut self.budget)?;
            self.budget.text(&generated, span)?;
            self.resolved_method_calls.insert(generated.clone(), calls);
        }
        self.budget.item(&instance)?;
        // Inspect complete substituted value fields while their ordinary
        // template identity exists. Explicit Managed arguments remain owning
        // values; this never grants Copy or manufactures a resource header.
        if let ItemKind::Enum(e) = &mut instance.kind
            && template != options::TEMPLATE
            && template != options::TASK_POLL
            && template != options::TASK_THREAD_SCOPE
        {
            let projected_bounds = if e.generics.iter().any(|parameter| {
                matches!(&parameter.kind, GenericParamKind::Type(bounds)
                    if bounds.iter().any(|bound| trait_instances::head(bound).is_some()))
            }) {
                Some(
                    self.traits
                        .parameter_bounds(&e.generics, &mut self.budget)?,
                )
            } else {
                None
            };
            let projection_variables = e
                .generics
                .iter()
                .filter(|parameter| matches!(parameter.kind, GenericParamKind::Type(_)))
                .map(|parameter| parameter.name.clone())
                .collect::<BTreeSet<_>>();
            for variant in &mut e.variants {
                if let VariantFields::Tuple(fields) = &mut variant.fields {
                    for field in fields {
                        if field.manages {
                            return Err(unsupported(field.span, "owned generic enum payload"));
                        }
                        if !stores::explicit_managed_value(
                            &field.ty,
                            &self.nominals,
                            &mut self.budget,
                        )? {
                            if projected_bounds.is_some()
                                && trait_bounds::associated::contains(&field.ty, &mut self.budget)?
                            {
                                let available = projected_bounds.as_ref().ok_or_else(|| {
                                    error(field.span, "enum projection has no declared trait bound")
                                })?;
                                field.ty = self.traits.normalize_associated(
                                    &field.ty,
                                    available,
                                    &projection_variables,
                                    &mut self.budget,
                                )?;
                                if trait_bounds::associated::contains(&field.ty, &mut self.budget)?
                                {
                                    copy::require(
                                        &field.ty,
                                        &self.nominals,
                                        available,
                                        &projection_variables,
                                        &mut self.budget,
                                    )?;
                                    continue;
                                }
                            }
                            copy::concrete_storage(&field.ty, &self.nominals, &mut self.budget)?;
                        }
                    }
                } else if let VariantFields::Record(fields) = &mut variant.fields {
                    for field in fields {
                        self.budget.node(field.span, 0)?;
                        if !stores::explicit_managed_value(
                            &field.ty,
                            &self.nominals,
                            &mut self.budget,
                        )? {
                            if projected_bounds.is_some()
                                && trait_bounds::associated::contains(&field.ty, &mut self.budget)?
                            {
                                let available = projected_bounds.as_ref().ok_or_else(|| {
                                    error(field.span, "enum projection has no declared trait bound")
                                })?;
                                field.ty = self.traits.normalize_associated(
                                    &field.ty,
                                    available,
                                    &projection_variables,
                                    &mut self.budget,
                                )?;
                                if trait_bounds::associated::contains(&field.ty, &mut self.budget)?
                                {
                                    copy::require(
                                        &field.ty,
                                        &self.nominals,
                                        available,
                                        &projection_variables,
                                        &mut self.budget,
                                    )?;
                                    continue;
                                }
                            }
                            copy::concrete_storage(&field.ty, &self.nominals, &mut self.budget)?;
                        }
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
                f.where_clause.clear();
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
        if (self.budget.has_anonymous_sums() || self.budget.has_type_member_patterns())
            && matches!(instance.kind, ItemKind::Function(_))
        {
            self.budget.text(&generated, span)?;
            self.budget.node(span, 0)?;
            // Transfer the already bounded substitution map; no second clone.
            self.anonymous_substitutions
                .insert(generated.clone(), substitution);
        }
        let origin = self.template_origin(item)?;
        self.enqueue(instance, origin)?;
        self.pending
            .back_mut()
            .expect("just enqueued")
            .runtime_calls = Some(runtime_calls);
        Ok(store_templates::Instantiation::Eager(generated))
    }
    fn ty(&mut self, ty: &mut TypeRef, depth: usize) -> R<()> {
        self.tick(ty.span, depth)?;
        if matches!(ty.kind, TypeKind::ForLifetime { .. })
            && self.prepare_rich_pointer(ty, depth + 1)?
        {
            return Ok(());
        }
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
                if self.stored_nominals
                    && path.len() == 1
                    && path[0] != options::TASK_THREAD_SCOPE
                    && self.stored_application(&path[0], args, ty.span, depth + 1)?
                {
                    let (name, _) = self.stored_nominal(&path[0], args, ty.span, depth + 1)?;
                    *path = vec![name];
                    args.clear();
                    return Ok(());
                }
                let mut values = Vec::new();
                let declaration = (path.len() == 1)
                    .then(|| self.templates.get(&path[0]).copied())
                    .flatten();
                for (index, arg) in args.iter_mut().enumerate() {
                    let parameter = declaration.and_then(|item| {
                        params(item)
                            .iter()
                            .filter(|parameter| {
                                !matches!(parameter.kind, GenericParamKind::Lifetime)
                            })
                            .nth(index)
                    });
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
        let old_life_scope = std::mem::take(&mut self.life_scope);
        if self.stored_nominals
            && let ItemKind::Function(function) = &item.kind
        {
            for parameter in &function.generics {
                if matches!(parameter.kind, GenericParamKind::Lifetime) {
                    self.budget.text(&parameter.name, parameter.span)?;
                    self.life_scope.push(parameter.name.clone());
                }
            }
        }
        let mut frame = BTreeMap::new();
        if self.traits.has_generic_methods
            && let ItemKind::Function(function) = &item.kind
            && let Some(calls) = self.resolved_method_calls.get(&function.name)
        {
            let ids = trait_instances::expression_ids(&function.body, &mut self.budget)?;
            let mut resolved = trait_instances::resolve_generic_calls(
                calls,
                &BTreeMap::new(),
                &mut self.budget,
                item.span,
            )?;
            for (pointer, ordinal) in ids {
                self.budget.node(item.span, 0)?;
                if let Some(target) = resolved.remove(&ordinal) {
                    frame.insert(pointer, target);
                }
            }
        }
        let previous = std::mem::replace(&mut self.method_calls, frame);
        let result = self.item_inner(item);
        self.method_calls = previous;
        self.life_scope = old_life_scope;
        result
    }
    fn item_inner(&mut self, item: &mut Item) -> R<()> {
        match &mut item.kind {
            ItemKind::Function(f) => {
                if self.stored_signature(f)? {
                    self.budget.function_snapshot(f)?;
                    self.budget.text(&f.name, f.span)?;
                    self.stored_lookup_fee(self.signatures.len(), f.name.len(), f.span)?;
                    self.signatures
                        .insert(f.name.clone(), Cow::Owned(f.clone()));
                }
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
                let callable = callable::template(f, &mut self.budget)?;
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
                            if let Some(code) = rich_pointer::role_code(bound, &mut self.budget)? {
                                self.prepare_rich_pointer(&code, 0)?;
                                continue;
                            }
                            let bound = callable::bound_inner_mut(bound);
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
                let mut env = Env::parameters(&f.params, f.span)?;
                if let Some(source) = self.stored_source_parameters.get(&f.name) {
                    self.budget.text(&f.name, f.span)?;
                    env.domain = f.name.clone();
                    for (parameter, source) in f.params.iter().zip(source) {
                        if let Some(source) = source {
                            self.budget
                                .ty(source.application.as_ref().expect("semantic source"))?;
                            self.budget.text(&source.domain, f.span)?;
                            self.budget.identity_work(source.classes.len(), f.span)?;
                            env.bindings
                                .get_mut(&parameter.name)
                                .expect("parameter binding")
                                .semantic = Some(source.clone());
                        }
                    }
                } else if self.stored_nominals {
                    self.budget.text(&f.name, f.span)?;
                    env.domain = f.name.clone();
                }
                let source_enum_result = self.source_enum_return.is_some();
                let body_result = self
                    .source_enum_return
                    .take()
                    .or_else(|| f.return_type.clone())
                    .unwrap_or_else(|| check::unit(f.span));
                let saved_result = self.return_type.replace(body_result);
                let saved_source_enum_result =
                    std::mem::replace(&mut self.source_enum_return_active, source_enum_result);
                let saved_callables = std::mem::replace(&mut self.active_callables, callable);
                match &mut f.body {
                    FunctionBody::Block(body) => self.block(body, &mut env, 0)?,
                    FunctionBody::Expression(value) => {
                        if self.source_enum_return_active
                            && let Some(result) = &self.return_type
                        {
                            self.budget.ty(result)?;
                        }
                        let expected = self.return_type.clone();
                        self.expr_expected(value, &mut env, expected.as_ref(), 0)?;
                    }
                    FunctionBody::Declaration => {}
                }
                self.return_type = saved_result;
                self.source_enum_return_active = saved_source_enum_result;
                self.active_callables = saved_callables;
                self.callable_name = saved_callable;
                self.callable_pair = saved_pair;
                self.store_parameter = saved_store;
                self.product_owner = saved_product_owner;
                self.anonymous_function = saved_anonymous;
                if self.stored_nominals {
                    for name in self.products.storage.functions.keys() {
                        self.budget.text(name, f.span)?;
                        self.budget.text(&f.name, f.span)?;
                    }
                    if self.products.storage.functions.contains_key(&f.name) {
                        stored_nominal::erase_stored_header_lifetimes(f);
                        f.generics.retain(|parameter| {
                            !matches!(parameter.kind, GenericParamKind::Lifetime)
                        });
                    }
                }
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
                for (ordinal, item) in a.methods.iter_mut().enumerate() {
                    let parent = self.current_origin;
                    let member = match (self.origins, parent) {
                        (Some(origins), Some(parent)) => Some(origins.aggregate_member(
                            parent,
                            ordinal,
                            &mut self.budget,
                            item.span,
                        )?),
                        (_, Some(parent)) => Some(parent.member(ordinal, item.span)?),
                        _ => None,
                    };
                    self.with_origin(member, |engine| engine.item(item))?;
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
                // Concrete scalar/enum Self is needed while checking the
                // ordinary body (including enum patterns), before Core lowers
                // receivers. No new premise is inferred from the host's shape.
                let concrete_value_host = trait_instances::head(&i.target).is_some_and(|host| {
                    super::static_traits::scalar_self(host) || self.enums.contains_key(host)
                });
                let source_target = if (self.traits.has_associated || concrete_value_host)
                    && !i
                        .trait_type
                        .as_ref()
                        .and_then(trait_instances::head)
                        .is_some_and(super::static_traits::reserved_trait)
                {
                    self.budget.ty(&i.target)?;
                    self.budget.text("Self", item.span)?;
                    let substitutions = BTreeMap::from([("Self".into(), i.target.clone())]);
                    for (ordinal, member) in i.members.iter_mut().enumerate() {
                        let anchors = self
                            .current_member_calls
                            .remove(&ordinal)
                            .map(|calls| {
                                trait_instances::anchor_resolved_calls(
                                    member,
                                    calls,
                                    &mut self.budget,
                                )
                            })
                            .transpose()?;
                        self.budget.substituted_item(member, &substitutions)?;
                        substitute_item(member, &substitutions)?;
                        if let Some(anchors) = anchors {
                            let calls = trait_instances::rekey_anchored_calls(
                                member,
                                anchors,
                                &mut self.budget,
                            )?;
                            self.current_member_calls.insert(ordinal, calls);
                        }
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
                            if !actual.generics.is_empty() {
                                continue;
                            }
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
                for (ordinal, item) in i.members.iter_mut().enumerate() {
                    if matches!(&item.kind,ItemKind::Function(function) if !function.generics.is_empty())
                    {
                        continue;
                    }
                    let origin = self
                        .current_origin
                        .map(|origin| origin.member(ordinal, item.span))
                        .transpose()?;
                    let calls = self.current_member_calls.remove(&ordinal);
                    let function_name = if calls.is_some() {
                        if let ItemKind::Function(function) = &item.kind {
                            self.budget.text(&function.name, item.span)?;
                            Some(function.name.clone())
                        } else {
                            None
                        }
                    } else {
                        None
                    };
                    let previous = if let (Some(name), Some(calls)) = (&function_name, calls) {
                        self.budget.text(name, item.span)?;
                        self.resolved_method_calls.insert(name.clone(), calls)
                    } else {
                        None
                    };
                    let result = self.with_origin(origin, |engine| engine.item(item));
                    if let Some(name) = function_name {
                        if let Some(previous) = previous {
                            self.resolved_method_calls.insert(name, previous);
                        } else {
                            self.resolved_method_calls.remove(&name);
                        }
                    }
                    result?;
                }
                if self.traits.has_generic_methods {
                    for member in &i.members {
                        self.budget.node(member.span, 0)?;
                    }
                    i.members.retain(|member|!matches!(&member.kind,ItemKind::Function(function) if !function.generics.is_empty()));
                }
                if self.traits.has_associated {
                    i.members.retain(|member| {
                        !matches!(member.kind, ItemKind::TypeAlias(_) | ItemKind::Const(_))
                    });
                }
            }
            ItemKind::Trait(declaration) if self.traits.has_generic_methods => {
                for member in &declaration.members {
                    self.budget.node(member.span, 0)?;
                }
                declaration.members.retain(|member|!matches!(&member.kind,ItemKind::Function(function) if !function.generics.is_empty()));
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
                let source_enum = if let Some(t) = ty.as_ref()
                    && self.stored_nominals
                    && let TypeKind::Named { path, args } = &grouped(t).kind
                    && path.len() == 1
                    && {
                        self.stored_lookup_fee(self.enums.len(), path[0].len(), t.span)?;
                        self.enums.contains_key(&path[0])
                            || matches!(
                                self.nominals.get(&path[0]),
                                Some(Item {
                                    kind: ItemKind::Aggregate(_),
                                    ..
                                })
                            )
                    }
                    && self.stored_application(&path[0], args, t.span, depth + 1)?
                {
                    self.budget.ty(t)?;
                    Some(t.clone())
                } else {
                    None
                };
                if let Some(t) = ty {
                    self.ty(t, depth + 1)?;
                }
                let inferred = self.expr_expected(
                    value,
                    env,
                    source_enum.as_ref().or(ty.as_ref()),
                    depth + 1,
                )?;
                let actual = source_enum.or_else(|| ty.clone()).or(inferred);
                if matches!(pattern.kind, PatternKind::RecordConstructor { .. }) {
                    self.enum_pattern(pattern, actual.as_ref(), env, depth + 1)?;
                } else {
                    bind(pattern, actual, env);
                    self.bind_semantic_source(pattern, env)?;
                }
            }
            StmtKind::Return(Some(e)) => {
                if self.source_enum_return_active
                    && let Some(result) = &self.return_type
                {
                    self.budget.ty(result)?;
                }
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
                Ok([
                    "u8", "u16", "u32", "u64", "usize", "i8", "i16", "i32", "i64", "isize",
                ]
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
            ExprKind::Unary {
                op: op @ (UnaryOp::Borrow | UnaryOp::BorrowMut),
                operand,
            } => Ok(self
                .value_hint(operand, env, depth + 1, default_integer)?
                .map(|inner| TypeRef {
                    span: e.span,
                    kind: TypeKind::Reference {
                        mutable: *op == UnaryOp::BorrowMut,
                        lifetime: None,
                        inner: Box::new(inner),
                    },
                })),
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
                            return self.stored_record_field_hint(&original, member);
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
                        if decl
                            .generics
                            .iter()
                            .any(|parameter| matches!(parameter.kind, GenericParamKind::Lifetime))
                        {
                            return enums::source_field_actuals(
                                &decl.generics,
                                args,
                                &field.ty,
                                &mut self.budget,
                            )
                            .map(Some);
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
        if let Some(ty) = self.expr_constant_type(e, env, depth)? {
            return self.expr_expected(e, env, Some(&ty), depth + 1);
        }
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
        let context = ExprExpectedContext {
            expected,
            error_context,
            anonymous_context,
            store_success_context,
        };
        self.expr_dispatch(e, env, context, depth)
    }

    #[inline(never)]
    fn expr_dispatch(
        &mut self,
        e: &mut Expr,
        env: &mut Env,
        context: ExprExpectedContext<'_>,
        depth: usize,
    ) -> R<Option<TypeRef>> {
        match &mut e.kind {
            ExprKind::Ascribed { .. } => self.expr_variant_ascribed(e, env, depth),
            ExprKind::Group(inner) | ExprKind::MetaEval(inner) => self.expr_expected(
                inner,
                env,
                context.expected.or(context.error_context),
                depth + 1,
            ),
            ExprKind::Unary { .. } => self.expr_variant_unary(e, env, context, depth),
            ExprKind::Binary { .. } => self.expr_variant_binary(e, env, context, depth),
            ExprKind::Call { .. } => self.expr_call_normal(e, env, context, depth),
            ExprKind::Tuple(..) => self.expr_variant_tuple(e, env, context, depth),
            ExprKind::Array(..) => self.expr_variant_array(e, env, context, depth),
            ExprKind::Member { .. } => self.expr_variant_member(e, env, context, depth),
            ExprKind::Index { .. } => self.expr_variant_index(e, env, context, depth),
            ExprKind::Slice { .. } => self.expr_variant_slice(e, env, context, depth),
            ExprKind::If { .. } => self.expr_variant_if(e, env, context, depth),
            ExprKind::Record { .. } => self.expr_variant_record(e, env, context, depth),
            ExprKind::Closure { .. } => self.expr_variant_closure(e, env, context, depth),
            ExprKind::MetaBlock(..) => self.expr_variant_meta_block(e, env, context, depth),
            ExprKind::Pipeline { .. } => self.expr_variant_pipeline(e, env, context, depth),
            ExprKind::Try(inner) => self.expr_try_result(inner, env, depth),
            _ => self.expr_leaf_result(e, env, context.expected),
        }
    }

    #[inline(never)]
    fn expr_try_result(
        &mut self,
        inner: &mut Expr,
        env: &mut Env,
        depth: usize,
    ) -> R<Option<TypeRef>> {
        let result = match self.expr(inner, env, depth + 1)? {
            Some(TypeRef {
                kind: TypeKind::Error { result, .. },
                ..
            }) => Some(*result),
            _ => None,
        };
        Ok(result)
    }

    #[inline(never)]
    fn expr_leaf_result(
        &mut self,
        e: &mut Expr,
        env: &mut Env,
        expected: Option<&TypeRef>,
    ) -> R<Option<TypeRef>> {
        let result = match &mut e.kind {
            ExprKind::Literal(Literal::Integer(text)) => {
                if let Some(ty) = expected.filter(|ty| check::integer(ty)) {
                    self.budget.ty(ty)?;
                    Some(ty.clone())
                } else {
                    Some(named(
                        if text.ends_with("u8") {
                            "u8"
                        } else if text.ends_with("u32") {
                            "u32"
                        } else if text.ends_with("i8") {
                            "i8"
                        } else if text.ends_with("i16") {
                            "i16"
                        } else if text.ends_with("isize") {
                            "isize"
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
                self.record_closure_input(name, env, e.span)?;
                if let Some(Some(ty)) = env.get(name) {
                    self.budget.ty(ty)?;
                    Some(ty.clone())
                } else {
                    None
                }
            }
            _ => None,
        };
        Ok(result)
    }
    fn signature_result(&mut self, name: &str) -> R<Option<TypeRef>> {
        if !self.stored_nominals {
            return self.signature_result_inner(name);
        }
        // A callee's retained source result belongs to its own life binders,
        // even when a caller with no life binder first requests its instance.
        // This supplies source type meaning only; call permissions come from
        // the actual stored signature and independently checked receiver.
        let mut scope = Vec::new();
        if let Some(function) = self.signatures.get(name) {
            for parameter in &function.generics {
                self.budget.node(parameter.span, 0)?;
                if matches!(parameter.kind, GenericParamKind::Lifetime) {
                    self.budget.text(&parameter.name, parameter.span)?;
                    scope.push(parameter.name.clone());
                }
            }
        }
        if scope.is_empty() {
            return self.signature_result_inner(name);
        }
        let previous = std::mem::replace(&mut self.life_scope, scope);
        let result = self.signature_result_inner(name);
        self.life_scope = previous;
        result
    }
    fn signature_result_inner(&mut self, name: &str) -> R<Option<TypeRef>> {
        let callable = self
            .signatures
            .get(name)
            .map(|f| callable::template(f, &mut self.budget))
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
        self.budget.text(&env.domain, Span::default())?;
        for binding in env.bindings.values() {
            if let Some(source) = &binding.semantic {
                self.budget
                    .ty(source.application.as_ref().expect("semantic source"))?;
                self.budget.text(&source.domain, Span::default())?;
                self.budget
                    .identity_work(source.classes.len(), Span::default())?;
                if source.domain != env.domain {
                    return Err(error(
                        Span::default(),
                        "semantic Type actual crossed its lexical domain without alpha-renaming",
                    ));
                }
            }
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
                        GenericArg::Lifetime(name) => {
                            budget.text(name, ty.span)?;
                            Ok(GenericArg::Lifetime(name.clone()))
                        }
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
    if let Some(key) = rich_pointer::key(ty, budget)? {
        return Ok(key);
    }
    // The profile is recorded during the existing bounded AST walk. Requests
    // without anonymous sums retain their original key fees and traversal.
    if budget.has_anonymous_sums()
        || budget.has_borrowed_keys()
        || budget.has_type_member_patterns()
    {
        type_keys::prepay(ty, budget)?;
    }
    render_type_key(ty)
}
fn render_type_key(ty: &TypeRef) -> R<String> {
    Ok(match &ty.kind {
        TypeKind::Named { path, args } if path.as_slice() == ["void"] && args.is_empty() => {
            "()".into()
        }
        TypeKind::Named { path, args }
        | TypeKind::Dyn {
            path,
            args,
            bindings: _,
        } => {
            if matches!(&ty.kind, TypeKind::Dyn { bindings, .. } if !bindings.is_empty()) {
                return Err(unsupported(
                    ty.span,
                    "dyn binding needs its canonical identity consumer",
                ));
            }
            let prefix = if matches!(&ty.kind, TypeKind::Dyn { .. }) {
                'D'
            } else {
                'N'
            };
            let mut key = format!("{prefix}{}:{}", path.join(".").len(), path.join("."));
            for a in args {
                let argument = match a {
                    GenericArg::Lifetime(_) => {
                        return Err(unsupported(
                            ty.span,
                            "lifetime instance identity requires scoped canonical storage relation",
                        ));
                    }
                    GenericArg::Type(t) => render_type_key(t)?,
                    GenericArg::Const(e) => format!("#{}", consts::key(e)?),
                };
                key.push_str(&format!("<{argument}>"));
            }
            key
        }
        TypeKind::Group(t) => return render_type_key(t),
        TypeKind::ForLifetime { .. } => return render_borrowed_type_key(ty),
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
/// Complete source identity for the canonical joint invocation relation.
/// The key planner prepays binder lookup, remapping and every rendered byte.
fn render_borrowed_type_key(ty: &TypeRef) -> R<String> {
    let mut names = Vec::new();
    let mut inner = ty;
    while let TypeKind::ForLifetime {
        lifetime,
        inner: next,
    } = &inner.kind
    {
        names.push(lifetime.as_str());
        inner = next;
    }
    let TypeKind::Function { params, result } = &inner.kind else {
        return Err(error(
            ty.span,
            "borrowed code key requires a function signature",
        ));
    };
    let mut remap = vec![None; names.len()];
    let mut count = 0u32;
    let mut label = |ty: &TypeRef| {
        let TypeKind::Reference {
            lifetime: Some(name),
            ..
        } = &grouped(ty).kind
        else {
            return None;
        };
        let old = names.iter().rposition(|candidate| *candidate == name)?;
        let dense = *remap[old].get_or_insert_with(|| {
            let next = count;
            count += 1;
            next
        });
        Some(dense)
    };
    let parameters: Vec<_> = params.iter().map(&mut label).collect();
    let result_life = label(result);
    if count == 0 {
        return render_type_key(inner);
    }
    let tag = |life: Option<u32>| life.map_or_else(|| "_".to_owned(), |n| n.to_string());
    let mut key = format!("B{count}[");
    for life in parameters {
        key.push_str(&tag(life));
        key.push(',');
    }
    key.push_str("]>");
    key.push_str(&tag(result_life));
    key.push(':');
    key.push_str(&render_type_key(inner)?);
    Ok(key)
}

fn unify(
    pattern: &TypeRef,
    actual: &TypeRef,
    variables: &BTreeSet<String>,
    found: &mut BTreeMap<String, TypeRef>,
    span: Span,
    budget: &mut Budget<'_>,
) -> R<()> {
    unify_scoped(pattern, actual, variables, found, span, budget, None)
}
fn unify_argument(
    pattern: &TypeRef,
    actual: &TypeRef,
    variables: &BTreeSet<String>,
    found: &mut BTreeMap<String, TypeRef>,
    span: Span,
    budget: &mut Budget<'_>,
) -> R<()> {
    unify_argument_scoped(pattern, actual, variables, found, span, budget, None)
}
fn unify_scoped(
    pattern: &TypeRef,
    actual: &TypeRef,
    variables: &BTreeSet<String>,
    found: &mut BTreeMap<String, TypeRef>,
    span: Span,
    budget: &mut Budget<'_>,
    source: Option<&Function>,
) -> R<()> {
    budget.ty(pattern)?;
    budget.ty(actual)?;
    unify_bounded(pattern, actual, variables, found, span, budget, source)
}
fn unify_argument_scoped(
    pattern: &TypeRef,
    actual: &TypeRef,
    variables: &BTreeSet<String>,
    found: &mut BTreeMap<String, TypeRef>,
    span: Span,
    budget: &mut Budget<'_>,
    source: Option<&Function>,
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
        if wanted == available
            && matches!(&expected.kind, TypeKind::Dyn { path, args, bindings } if path.len() == 1 && args.is_empty() && bindings.is_empty())
            && matches!(&actual.kind, TypeKind::Named { path, args } if path.len() == 1 && args.is_empty())
        {
            // This is only a syntactic candidate. The selected concrete
            // provider must come from the committed Source catalog, and Types
            // checks its exact table and real borrowed receiver before it
            // emits MakeDynBorrow. No generic substitution is inferred here.
            budget.node(span, 0)?;
            return Ok(());
        }
        return unify_bounded(expected, actual, variables, found, span, budget, source);
    }
    unify_bounded(pattern, actual, variables, found, span, budget, source)
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
    source: Option<&Function>,
) -> R<()> {
    if let TypeKind::Group(t) = &pattern.kind {
        return unify_bounded(t, actual, variables, found, span, budget, source);
    }
    if let TypeKind::Group(t) = &actual.kind {
        return unify_bounded(pattern, t, variables, found, span, budget, source);
    }
    if let TypeKind::Named { path, args } = &pattern.kind
        && path.len() == 1
        && args.is_empty()
        && variables.contains(&path[0])
    {
        if let Some(prior) = found.get(&path[0]) {
            if source_unification_key(prior, source, budget)?
                != source_unification_key(actual, source, budget)?
            {
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
            unify_bounded(a, b, variables, found, span, budget, source)?;
            unify_bounded(x, y, variables, found, span, budget, source)
        }
        (TypeKind::Named { path: a, args: x }, TypeKind::Named { path: b, args: y })
            if a == b && x.len() == y.len() =>
        {
            for (a, b) in x.iter().zip(y) {
                let (GenericArg::Type(a), GenericArg::Type(b)) = (a, b) else {
                    return Err(unsupported(span, "const generic unification"));
                };
                unify_bounded(a, b, variables, found, span, budget, source)?;
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
        ) if a == b => unify_bounded(x, y, variables, found, span, budget, source),
        (TypeKind::Tuple(a), TypeKind::Tuple(b)) if a.len() == b.len() => {
            for (a, b) in a.iter().zip(b) {
                unify_bounded(a, b, variables, found, span, budget, source)?;
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
            unify_bounded(a, b, variables, found, span, budget, source)
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
        ) => unify_bounded(a, b, variables, found, span, budget, source),
        _ if source_unification_key(pattern, source, budget)?
            == source_unification_key(actual, source, budget)? =>
        {
            Ok(())
        }
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
        ItemKind::Const(constant) => {
            if let Some(ty) = &mut constant.ty {
                visitor.ty(ty);
            }
            if let Some(value) = &mut constant.value {
                visitor.expr(value);
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
        if let TypeKind::Named { path, .. } = &mut ty.kind
            && path.len() > 1
            && let Some(replacement) = self
                .substitutions
                .get(&path[0])
                .and_then(trait_bounds::plain)
        {
            path[0] = replacement.to_owned();
        }
        match &mut ty.kind {
            TypeKind::Named { args, .. } | TypeKind::Dyn { args, .. } => {
                for a in args {
                    match a {
                        GenericArg::Lifetime(_) => {}
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
        if let ExprKind::Member {
            object,
            member: MemberName::Name(member),
        } = &e.kind
            && let ExprKind::Name(subject) = &object.kind
            && self
                .substitutions
                .get(subject)
                .is_some_and(trait_bounds::constants::subject)
            && let Some(value) = self
                .substitutions
                .get(&trait_bounds::constants::key(subject, member))
                .and_then(consts::expression)
        {
            let span = e.span;
            *e = value.clone();
            e.span = span;
            return;
        }
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
            && !self
                .substitutions
                .get(name)
                .is_some_and(trait_bounds::constants::subject)
            && let Some(value) = self.substitutions.get(name).and_then(consts::expression)
        {
            let span = e.span;
            *e = value.clone();
            e.span = span;
            return;
        }
        match &mut e.kind {
            ExprKind::Ascribed { ty, value } => {
                self.ty(ty);
                self.expr(value);
            }
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
                        GenericArg::Lifetime(_) => {}
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
#[path = "generic_error_injection.rs"]
mod error_injection;
use check::validate_parametric;

#[path = "generic_type_keys.rs"]
mod type_keys;

#[path = "generic_expr_variants.rs"]
mod expr_variants;
use expr_variants::ExprExpectedContext;

#[path = "generic_expr_call.rs"]
mod expr_call;
