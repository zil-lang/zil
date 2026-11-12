//! Project syntax normalization. Hygienic names are not type certificates: the
//! resulting declarations still enter ordinary shape, type and safety checks.
use super::*;
use generics::{Budget, trait_instances};
use std::borrow::Cow;
use zil_base::{FileId, ModuleId};
use zil_syntax::ast::*;

type R<T> = Result<T, Diagnostic>;
type Names = BTreeSet<String>;
// The flag distinguishes source declarations from actual specialization output;
// a spelling prefix alone is not evidence of an instantiated function.
type Functions = BTreeMap<String, (ModuleId, String, bool)>;

#[derive(Clone)]
struct Symbol {
    canonical: String,
    public: bool,
    function: bool,
    generic: bool,
}
struct Index {
    symbols: BTreeMap<ModuleId, BTreeMap<String, Symbol>>,
    imports: BTreeMap<ModuleId, BTreeMap<String, ModuleId>>,
    const_parameters: BTreeMap<String, Vec<bool>>,
}

fn const_slots(item: &Item, budget: &mut Budget<'_>) -> R<Vec<bool>> {
    let parameters = match &item.kind {
        ItemKind::Function(f) | ItemKind::Extern { function: f, .. } => f.generics.as_slice(),
        ItemKind::Aggregate(a) => a.generics.as_slice(),
        ItemKind::Enum(e) => e.generics.as_slice(),
        ItemKind::Trait(t) => t.generics.as_slice(),
        ItemKind::TypeAlias(a) => a.generics.as_slice(),
        _ => &[],
    };
    if !parameters
        .iter()
        .any(|p| matches!(p.kind, GenericParamKind::Const(_)))
    {
        return Ok(vec![]);
    }
    let mut slots = Vec::new();
    // Nominal applications retain lifetime actuals at their full ordinals.
    // Ordinary function instantiation currently elides its life-only formals.
    let function = matches!(item.kind, ItemKind::Function(_) | ItemKind::Extern { .. });
    for parameter in parameters {
        budget.node(parameter.span, 0)?;
        if !function || !matches!(parameter.kind, GenericParamKind::Lifetime) {
            slots.push(matches!(parameter.kind, GenericParamKind::Const(_)));
        }
    }
    Ok(slots)
}

/// A direct single-module API does not run project normalization. Protect any
/// nominal declaration that a later alias/function substitution could capture.
/// The caller has bounded and validated the complete borrowed syntax first.
pub(super) fn protect_alias_nominals<'a>(
    module: &'a Module,
    budget: &mut Budget<'_>,
) -> R<Cow<'a, Module>> {
    if !module
        .items
        .iter()
        .any(|item| matches!(item.kind, ItemKind::Aggregate(_) | ItemKind::Enum(_)))
    {
        return Ok(Cow::Borrowed(module));
    }
    fn binders<'a>(
        item: &'a Item,
        names: &mut BTreeSet<&'a str>,
        budget: &mut Budget<'_>,
        depth: usize,
    ) -> R<()> {
        budget.node(item.span, depth)?;
        let (parameters, members): (&[GenericParam], &[Item]) = match &item.kind {
            ItemKind::Function(f) | ItemKind::Extern { function: f, .. } => (&f.generics, &[]),
            ItemKind::Aggregate(a) => (&a.generics, &a.methods),
            ItemKind::Enum(e) => (&e.generics, &[]),
            ItemKind::TypeAlias(a) => (&a.generics, &[]),
            ItemKind::Impl(i) => (&i.generics, &i.members),
            ItemKind::Trait(t) => (&t.generics, &t.members),
            _ => (&[], &[]),
        };
        for parameter in parameters {
            budget.node(parameter.span, depth + 1)?;
            budget.text(&parameter.name, parameter.span)?;
            if matches!(
                parameter.kind,
                GenericParamKind::Type(_) | GenericParamKind::Const(_)
            ) {
                names.insert(&parameter.name);
            }
        }
        for member in members {
            binders(member, names, budget, depth + 1)?;
        }
        Ok(())
    }
    let mut parameters = BTreeSet::new();
    let mut occupied = BTreeSet::new();
    for item in &module.items {
        binders(item, &mut parameters, budget, 0)?;
        if let Some((name, _, _)) = declaration(item) {
            occupied.insert(name);
        }
    }
    let mut symbols = BTreeMap::new();
    let mut const_parameters = BTreeMap::new();
    for item in &module.items {
        let (name, generic) = match &item.kind {
            ItemKind::Aggregate(a) => (&a.name, !a.generics.is_empty()),
            ItemKind::Enum(e) => (&e.name, !e.generics.is_empty()),
            _ => continue,
        };
        if !parameters.contains(name.as_str()) {
            continue;
        }
        let mut suffix = 0usize;
        let canonical = loop {
            budget.node(item.span, 0)?;
            let candidate = format!("$alias_nominal{}_{}_{}", module.id.0, suffix, name);
            budget.text(&candidate, item.span)?;
            if !parameters.contains(candidate.as_str()) && !occupied.contains(candidate.as_str()) {
                break candidate;
            }
            suffix = suffix.checked_add(1).ok_or_else(|| {
                Diagnostic::limit(
                    Phase::Expand,
                    "alias nominal identity budget exhausted",
                    Some(item.span),
                )
            })?;
        };
        let slots = const_slots(item, budget)?;
        if !slots.is_empty() {
            budget.text(&canonical, item.span)?;
            const_parameters.insert(canonical.clone(), slots);
        }
        budget.text(name, item.span)?;
        symbols.insert(
            name.clone(),
            Symbol {
                canonical,
                public: item.public,
                function: false,
                generic,
            },
        );
    }
    if symbols.is_empty() {
        return Ok(Cow::Borrowed(module));
    }
    let index = Index {
        symbols: BTreeMap::from([(module.id, symbols)]),
        imports: BTreeMap::from([(module.id, BTreeMap::new())]),
        const_parameters,
    };
    // The borrowed preflight above precedes this full syntax clone. The same
    // lexical visitor as project normalization preserves local shadowing.
    let mut output = module.clone();
    let mut visitor = Visitor {
        default_nominal: None,
        const_scope: Names::new(),
        mode: Mode::Normalize(&index),
        module: module.id,
        budget,
    };
    for item in &mut output.items {
        visitor.item(item, &Names::new(), true, 0)?;
    }
    Ok(Cow::Owned(output))
}

/// Give each lexical type binder an identity separate from source spellings,
/// including builtin names. Definitions and all scoped references are rewritten
/// together before substitution; no previously legal binder becomes reserved.
pub(super) fn protect_alias_binders<'a>(
    module: &'a Module,
    budget: &mut Budget<'_>,
) -> R<Cow<'a, Module>> {
    fn collect(
        item: &Item,
        occupied: &mut Names,
        found: &mut bool,
        budget: &mut Budget<'_>,
        depth: usize,
    ) -> R<()> {
        budget.node(item.span, depth)?;
        if let Some((name, _, _)) = declaration(item) {
            budget.text(name, item.span)?;
            occupied.insert(name.to_owned());
        }
        let (params, members): (&[GenericParam], &[Item]) = match &item.kind {
            ItemKind::Function(f) | ItemKind::Extern { function: f, .. } => (&f.generics, &[]),
            ItemKind::Aggregate(a) => (&a.generics, &a.methods),
            ItemKind::Enum(e) => (&e.generics, &[]),
            ItemKind::TypeAlias(a) => (&a.generics, &[]),
            ItemKind::Impl(i) => (&i.generics, &i.members),
            ItemKind::Trait(t) => (&t.generics, &t.members),
            _ => (&[], &[]),
        };
        for p in params {
            budget.node(p.span, depth + 1)?;
            budget.text(&p.name, p.span)?;
            occupied.insert(p.name.clone());
            *found |= matches!(p.kind, GenericParamKind::Type(_));
        }
        for member in members {
            collect(member, occupied, found, budget, depth + 1)?;
        }
        Ok(())
    }
    let mut occupied = Names::new();
    let mut found = false;
    for item in &module.items {
        collect(item, &mut occupied, &mut found, budget, 0)?;
    }
    if !found {
        return Ok(Cow::Borrowed(module));
    }
    let mut output = module.clone();
    let mut visitor = Visitor {
        default_nominal: None,
        const_scope: Names::new(),
        mode: Mode::Binders {
            bindings: BTreeMap::new(),
            occupied,
            next: 0,
        },
        module: module.id,
        budget,
    };
    for item in &mut output.items {
        visitor.item(item, &Names::new(), true, 0)?;
    }
    Ok(Cow::Owned(output))
}
fn error(code: &str, span: Span, message: &str) -> Diagnostic {
    Diagnostic::error(Phase::Expand, code, Some(span), message)
}
fn declaration(item: &Item) -> Option<(&str, bool, bool)> {
    match &item.kind {
        ItemKind::Function(f) => Some((&f.name, true, !f.generics.is_empty())),
        ItemKind::Aggregate(a) => Some((&a.name, false, !a.generics.is_empty())),
        ItemKind::Enum(e) => Some((&e.name, false, !e.generics.is_empty())),
        ItemKind::TypeAlias(a) => Some((&a.name, false, !a.generics.is_empty())),
        ItemKind::Trait(t) => Some((&t.name, false, !t.generics.is_empty())),
        ItemKind::Const(c) => Some((&c.name, false, false)),
        ItemKind::Extern { function, .. } => Some((&function.name, true, false)),
        _ => None,
    }
}

#[cfg(test)]
pub(super) fn specialize(
    sources: &[SourceUnit],
    modules: &[&Module],
    imports: &[Vec<CoreImport>],
    limits: &Limits,
    products: &mut generics::products::Products,
) -> CompileResult<Vec<(Module, Vec<CoreImport>)>> {
    specialize_with_context(sources, modules, imports, limits, products, None)
}
pub(super) fn specialize_with_context(
    sources: &[SourceUnit],
    modules: &[&Module],
    imports: &[Vec<CoreImport>],
    limits: &Limits,
    products: &mut generics::products::Products,
    context: Option<super::source_context::SourceIdentityContext<'_>>,
) -> CompileResult<Vec<(Module, Vec<CoreImport>)>> {
    specialize_with_runtime(sources, modules, imports, limits, products, context, None)
}
pub(super) fn specialize_with_runtime(
    sources: &[SourceUnit],
    modules: &[&Module],
    imports: &[Vec<CoreImport>],
    limits: &Limits,
    products: &mut generics::products::Products,
    context: Option<super::source_context::SourceIdentityContext<'_>>,
    runtime_ct: Option<&dyn crate::runtime_ct::RuntimeCt>,
) -> CompileResult<Vec<(Module, Vec<CoreImport>)>> {
    normalize(
        sources, modules, imports, limits, products, context, runtime_ct,
    )
    .map_err(|e| vec![e])
}
fn normalize(
    sources: &[SourceUnit],
    modules: &[&Module],
    imports: &[Vec<CoreImport>],
    limits: &Limits,
    products: &mut generics::products::Products,
    context: Option<super::source_context::SourceIdentityContext<'_>>,
    runtime_ct: Option<&dyn crate::runtime_ct::RuntimeCt>,
) -> R<Vec<(Module, Vec<CoreImport>)>> {
    let mut budget = Budget::new(limits);
    if runtime_ct.is_some() {
        budget.note_source_identity();
    }
    if sources.len() != modules.len() || modules.len() != imports.len() || modules.is_empty() {
        return Err(error(
            "E-MODULE-ID",
            Span::default(),
            "inconsistent project syntax inputs",
        ));
    }
    // Bound borrowed inputs before cloning any syntax or symbol table.
    for ((source, module), edges) in sources.iter().zip(modules).zip(imports) {
        budget.node(module.span, 0)?;
        if source.module_id != module.id || !module.span.is_valid_for(source) {
            return Err(error(
                "E-MODULE-ID",
                module.span,
                "source and syntax module identities differ",
            ));
        }
        for item in &module.items {
            budget.item(item)?;
        }
        for edge in edges {
            budget.node(edge.span, 0)?;
            budget.text(&edge.alias, edge.span)?;
        }
    }
    // Feature preparation can discover a provenance consumer after project
    // modules have been combined. Record actual declaration ownership now;
    // the synthetic combined module is never a source declaration owner.
    let mut origins = Some(generics::origins::Origins::default());
    if budget.needs_source_identity()
        && let Some(context) = context
    {
        let resolved = context.resolve(sources, &mut budget)?;
        for module in modules {
            let mut snapshot = generics::origins::Origins::single(module, false, &mut budget)?;
            snapshot.capture_identities(module, false, &resolved, &mut budget)?;
            let output = origins.as_mut().expect("dyn provenance ledger");
            for (key, identity) in snapshot.identities {
                budget.lineage_bytes(
                    std::mem::size_of::<crate::source_context::DeclarationOrigin>(),
                    module.span,
                )?;
                for _ in 0..output
                    .identities
                    .len()
                    .checked_ilog2()
                    .map_or(1, |n| n as usize + 2)
                {
                    budget.node(module.span, 0)?;
                }
                if output.identities.insert(key, identity).is_some() {
                    return Err(error(
                        "E-SOURCE-IDENTITY",
                        module.span,
                        "duplicate source declaration context",
                    ));
                }
            }
        }
    }
    let mut index = Index {
        symbols: BTreeMap::new(),
        imports: BTreeMap::new(),
        const_parameters: BTreeMap::new(),
    };
    let mut files = BTreeMap::<FileId, usize>::new();
    let mut functions = Functions::new();
    for (position, ((source, module), edges)) in
        sources.iter().zip(modules).zip(imports).enumerate()
    {
        if files.insert(source.file_id, position).is_some()
            || index.symbols.contains_key(&module.id)
        {
            return Err(error(
                "E-MODULE-ID",
                module.span,
                "duplicate project module or file identity",
            ));
        }
        let mut symbols = BTreeMap::new();
        for item in &module.items {
            if let Some((name, function, generic)) = declaration(item) {
                if symbols.contains_key(name)
                    || super::primitive_name(name)
                    || matches!(name, "bool" | "void" | "c_handle")
                {
                    return Err(error(
                        "E-NAME-DUPLICATE",
                        item.span,
                        "duplicate declaration name",
                    ));
                }
                let canonical = format!("$module{}_{}", module.id.0, name);
                budget.text(&canonical, item.span)?;
                let slots = const_slots(item, &mut budget)?;
                if !slots.is_empty() {
                    budget.text(&canonical, item.span)?;
                    index.const_parameters.insert(canonical.clone(), slots);
                }
                if function {
                    functions.insert(canonical.clone(), (module.id, name.to_owned(), true));
                }
                symbols.insert(
                    name.to_owned(),
                    Symbol {
                        canonical,
                        public: item.public,
                        function,
                        generic,
                    },
                );
            }
        }
        let mut aliases = BTreeMap::new();
        for edge in edges {
            if edge.alias == "Option"
                || symbols.contains_key(&edge.alias)
                || aliases.insert(edge.alias.clone(), edge.module).is_some()
            {
                return Err(error(
                    "E-MODULE-ALIAS",
                    edge.span,
                    "module alias conflicts with another declaration",
                ));
            }
        }
        index.symbols.insert(module.id, symbols);
        index.imports.insert(module.id, aliases);
    }
    let mut static_hosts = Vec::new();
    let mut scalar_traits = BTreeSet::new();
    let mut combined = Module {
        id: modules[0].id,
        span: modules[0].span,
        items: Vec::new(),
    };
    for module in modules {
        let mut visitor = Visitor {
            default_nominal: None,
            const_scope: Names::new(),
            mode: Mode::Normalize(&index),
            module: module.id,
            budget: &mut budget,
        };
        for (ordinal, source_item) in module.items.iter().enumerate() {
            if matches!(source_item.kind, ItemKind::Import(_)) {
                continue;
            }
            if runtime_ct.is_some()
                && matches!(&source_item.kind, ItemKind::Aggregate(a) if !a.generics.is_empty() && !a.methods.is_empty())
            {
                let source_origin =
                    generics::origins::Origin::source(module.id, ordinal, source_item.span)?;
                let row = origins
                    .as_ref()
                    .and_then(|origins| origins.identities.get(&source_origin.declaration))
                    .ok_or_else(|| {
                        error(
                            "E-SOURCE-IDENTITY",
                            source_item.span,
                            "CT namespace original input identity absent",
                        )
                    })?;
                generics::origins::pay_identity(&row.identity, visitor.budget, source_item.span)?;
                generics::origins::pay_declaration_copy(row, visitor.budget, source_item.span)?;
                visitor.budget.lineage_bytes(
                    (static_hosts.len() + 1) * std::mem::size_of::<StaticHostInput<'_>>(),
                    source_item.span,
                )?;
                static_hosts.reserve_exact(1);
                static_hosts.push(StaticHostInput {
                    item: source_item,
                    definition: crate::runtime_ct::RuntimeDefinition {
                        module: module.id,
                        origin: row.clone(),
                        member: None,
                    },
                });
            }
            let mut item = source_item.clone();
            visitor.item(&mut item, &Names::new(), true, 0)?;
            if let ItemKind::Impl(implementation) = &item.kind
                && trait_instances::head(&implementation.target)
                    .is_some_and(super::static_traits::scalar_self)
                && let Some(name) = implementation
                    .trait_type
                    .as_ref()
                    .and_then(trait_instances::head)
            {
                visitor.budget.text(name, item.span)?;
                visitor.budget.node(item.span, 0)?;
                scalar_traits.insert(name.to_owned());
            }
            if let Some(origins) = &mut origins {
                visitor.budget.node(item.span, 0)?;
                origins.entries.push(generics::origins::Origin::source(
                    module.id, ordinal, item.span,
                )?);
            }
            combined.items.push(item);
        }
    }
    let mut has_trait_instances = false;
    let mut static_trait_methods = BTreeMap::<String, BTreeSet<String>>::new();
    for item in &combined.items {
        if let ItemKind::Trait(t) = &item.kind {
            // This is the same paid member walk as features_with_methods.
            // Record receiver-free declarations and actual scalar providers,
            // whose calls normalize to direct functions. An impl may omit
            // `self` while inheriting a receiver from its trait signature.
            let mut defaults = false;
            let mut associated = false;
            let mut methods = false;
            for member in &t.members {
                budget.node(member.span, 0)?;
                associated |= matches!(member.kind, ItemKind::TypeAlias(_) | ItemKind::Const(_));
                if let ItemKind::Function(function) = &member.kind {
                    methods |= !function.generics.is_empty();
                    defaults |= !matches!(function.body, FunctionBody::Declaration);
                    if function.generics.is_empty()
                        && (!function.params.first().is_some_and(|p| p.name == "self")
                            || scalar_traits.contains(&t.name))
                    {
                        budget.text(&t.name, item.span)?;
                        budget.text(&function.name, member.span)?;
                        budget.lineage_bytes(
                            std::mem::size_of::<(String, BTreeSet<String>)>()
                                .saturating_add(t.name.len())
                                .saturating_add(function.name.len()),
                            member.span,
                        )?;
                        static_trait_methods
                            .entry(t.name.clone())
                            .or_default()
                            .insert(function.name.clone());
                    }
                }
            }
            has_trait_instances |=
                defaults || associated || methods || !t.generics.is_empty() || !t.bounds.is_empty();
        }
        has_trait_instances |= matches!(&item.kind, ItemKind::Impl(i)
            if i.trait_type.is_some() && (!i.generics.is_empty() || !i.where_clause.is_empty()
                || !matches!(&i.target.kind, TypeKind::Named { path, args } if path.len()==1 && args.is_empty())
                || trait_instances::head(&i.target).is_some_and(super::static_traits::scalar_self)));
    }
    has_trait_instances |= budget.has_enum_profiles()
        || budget.has_primitive_binders()
        || budget.needs_source_identity();
    let instance_limits = if has_trait_instances {
        budget.remaining_limits()
    } else {
        limits.clone()
    };
    products.origins = origins;
    let runtime_bridge = runtime_ct.map(|callback| NormalizedRuntimeCt {
        callback,
        index: &index,
        static_hosts: &static_hosts,
    });
    let specialized = match generics::specialize_products_with_runtime(
        &combined,
        &instance_limits,
        products,
        None,
        runtime_bridge
            .as_ref()
            .map(|bridge| bridge as &dyn crate::runtime_ct::RuntimeCt),
    )
    .map_err(|mut e| e.remove(0))?
    {
        // A borrowed result is the unchanged combined input. Keep that owned
        // syntax rather than cloning it again while redistributing modules.
        Cow::Borrowed(_) => combined,
        Cow::Owned(module) => module,
    };
    if has_trait_instances && let Some(remaining) = &products.allowance {
        budget.adopt_remaining(remaining);
    }
    let mut output = modules
        .iter()
        .zip(imports)
        .map(|(m, i)| {
            (
                Module {
                    id: m.id,
                    span: m.span,
                    items: Vec::new(),
                },
                i.clone(),
            )
        })
        .collect::<Vec<_>>();
    let mut has_project_trait_impl = false;
    let mut positions = Vec::new();
    let mut static_impls = Vec::new();
    for (ordinal, item) in specialized.items.iter().enumerate() {
        budget.item(item)?;
        let position = if let Some(origins) = &products.origins {
            budget.node(item.span, 0)?;
            let owner = origins.get(ordinal, item.span)?.declaration.module;
            let mut position = None;
            for (index, module) in modules.iter().enumerate() {
                budget.node(item.span, 0)?;
                if module.id == owner {
                    position = Some(index);
                    break;
                }
            }
            position.ok_or_else(|| {
                error(
                    "E-MODULE-ID",
                    item.span,
                    "explicit declaration owner absent",
                )
            })?
        } else {
            *files.get(&item.span.file).ok_or_else(|| {
                error(
                    "E-MODULE-ID",
                    item.span,
                    "instantiated declaration lost its definition source",
                )
            })?
        };
        if products.origins.is_some() {
            budget.node(item.span, 0)?;
            positions.push(position);
        }
        has_project_trait_impl |= project_traits::is_external_impl(item, modules[position].id);
        if let ItemKind::Function(f) = &item.kind {
            // An instance stays in its template's defining module. Publicness
            // is inherited from that declaration, never from the consumer.
            functions.entry(f.name.clone()).or_insert((
                modules[position].id,
                f.name.clone(),
                false,
            ));
        }
        if let ItemKind::Impl(implementation) = &item.kind
            && let Some(trait_name) = implementation
                .trait_type
                .as_ref()
                .and_then(trait_instances::head)
            && let Some(static_members) = static_trait_methods.get(trait_name)
            && implementation.members.iter().any(|member| {
                matches!(&member.kind,
                ItemKind::Function(method) if method.generics.is_empty()
                    && static_members.contains(&method.name))
            })
        {
            budget.lineage_bytes(std::mem::size_of::<(&Item, ModuleId)>(), item.span)?;
            static_impls.push((item, modules[position].id));
        }
    }
    if !static_impls.is_empty() {
        // These entries describe actual impl members that the static collector
        // will emit. Restore uses the owner to form an ordinary import; it
        // does not infer a provider or visibility from the generated spelling.
        let mut declarations = BTreeMap::new();
        for item in &specialized.items {
            budget.node(item.span, 0)?;
            if let ItemKind::Trait(declaration) = &item.kind {
                budget.text(&declaration.name, item.span)?;
                declarations.insert(declaration.name.as_str(), (item, declaration));
            }
        }
        for (item, owner) in static_impls {
            let ItemKind::Impl(implementation) = &item.kind else {
                unreachable!()
            };
            let application = implementation.trait_type.as_ref().expect("static impl");
            let Some(trait_name) = trait_instances::head(application) else {
                continue;
            };
            budget.text(trait_name, application.span)?;
            let Some((trait_item, declaration)) = declarations.get(trait_name).copied() else {
                continue;
            };
            let Some(host) = trait_instances::head(&implementation.target) else {
                continue;
            };
            let reference =
                trait_instances::arguments(trait_item.span, declaration, application, &mut budget)?;
            let instanced = !declaration.generics.is_empty()
                || declaration.members.iter().any(|member| {
                    matches!(member.kind, ItemKind::TypeAlias(_) | ItemKind::Const(_))
                });
            for member in &implementation.members {
                budget.node(member.span, 0)?;
                let ItemKind::Function(method) = &member.kind else {
                    continue;
                };
                if !method.generics.is_empty()
                    || !static_trait_methods
                        .get(trait_name)
                        .is_some_and(|names| names.contains(&method.name))
                {
                    continue;
                }
                let scalar = super::static_traits::scalar_self(host);
                let target = if instanced || scalar {
                    trait_instances::method_name_with(
                        &reference,
                        &method.name,
                        scalar,
                        &mut budget,
                        member.span,
                    )?
                } else {
                    budget.text(&method.name, member.span)?;
                    method.name.clone()
                };
                budget.text(host, member.span)?;
                budget.text(".", member.span)?;
                budget.text(&target, member.span)?;
                budget.lineage_bytes(
                    std::mem::size_of::<(String, (ModuleId, String, bool))>()
                        .saturating_add(2 * (host.len() + 1 + target.len())),
                    member.span,
                )?;
                let key = format!("{host}.{target}");
                functions.entry(key.clone()).or_insert((owner, key, false));
            }
        }
    }
    let method_owners = if has_project_trait_impl {
        project_traits::method_owners(&specialized, modules, &files, &mut budget)?
    } else {
        BTreeMap::new()
    };
    // All source identities, budget visits and project-method checks above
    // retain their original order. No fallible check depends on the copies;
    // transfer each declaration only after those checks have succeeded.
    let mut signature_candidates=std::mem::take(&mut products.storage.signature_candidates);
    for (ordinal, item) in specialized.items.into_iter().enumerate() {
        let position = if products.origins.is_some() {
            positions[ordinal]
        } else {
            files[&item.span.file]
        };
        if let Some(origins) = &mut products.origins
            && !origins.identities.is_empty()
        {
            let origin = origins.get(ordinal, item.span)?;
            for _ in 0..origins
                .modules
                .len()
                .checked_ilog2()
                .map_or(1, |n| n as usize + 2)
            {
                budget.node(item.span, 0)?;
            }
            budget.node(item.span, 0)?;
            origins
                .modules
                .entry(output[position].0.id)
                .or_default()
                .push(origin);
        }
        if let Some(candidate)=signature_candidates.remove(&(specialized.id,ordinal)) {
            budget.node(item.span,0)?;
            let key=(output[position].0.id,output[position].0.items.len());
            products.storage.signature_candidates.insert(key,candidate);
        }
        output[position].0.items.push(item);
    }
    for (module, edges) in &mut output {
        if !method_owners.is_empty() {
            for item in &module.items {
                budget.node(item.span, 0)?;
                let ItemKind::Function(function) = &item.kind else {
                    continue;
                };
                budget.text(&function.name, item.span)?;
                if functions
                    .get(&function.name)
                    .is_some_and(|(_, _, original)| !original)
                {
                    project_traits::instance_dependencies(
                        item,
                        &method_owners,
                        module.id,
                        edges,
                        &mut budget,
                    )?;
                }
            }
        }
        let mut visitor = Visitor {
            default_nominal: None,
            const_scope: Names::new(),
            mode: Mode::Restore {
                functions: &functions,
                edges,
            },
            module: module.id,
            budget: &mut budget,
        };
        for item in &mut module.items {
            visitor.item(item, &Names::new(), true, 0)?;
        }
    }
    products
        .storage
        .restore_functions(&functions, &mut budget)?;
    if has_trait_instances {
        products.allowance = Some(budget.remaining_limits());
    }
    products.restore_owners(&functions)?;
    Ok(output)
}

/// Re-enter the actual module normalizer for syntax produced by the CT
/// consumer. Its original source names cannot be interpreted as already
/// normalized template names by the generic engine.
struct NormalizedRuntimeCt<'a> {
    callback: &'a dyn crate::runtime_ct::RuntimeCt,
    index: &'a Index,
    static_hosts: &'a [StaticHostInput<'a>],
}
struct StaticHostInput<'a> {
    item: &'a Item,
    definition: crate::runtime_ct::RuntimeDefinition,
}
impl crate::runtime_ct::RuntimeCt for NormalizedRuntimeCt<'_> {
    fn check_static_host(
        &self,
        definition: &crate::runtime_ct::RuntimeDefinition,
        actual: &Item,
        remaining: &mut Limits,
    ) -> CompileResult<()> {
        let mut budget = Budget::new(remaining);
        budget.limit_temporary_bytes(remaining.max_source_bytes);
        let result = (|| -> R<&Item> {
            let mut selected = None;
            for input in self.static_hosts {
                generics::origins::pay_identity(
                    &input.definition.origin.identity,
                    &mut budget,
                    actual.span,
                )?;
                generics::origins::pay_identity(
                    &definition.origin.identity,
                    &mut budget,
                    actual.span,
                )?;
                generics::origins::pay_declaration_copy(
                    &input.definition.origin,
                    &mut budget,
                    actual.span,
                )?;
                let left = &input.definition.origin;
                let right = &definition.origin;
                let generated = match (&left.generated, &right.generated) {
                    (None, None) => true,
                    (Some(l), Some(r)) => {
                        l.root == r.root && std::sync::Arc::ptr_eq(&l.tree, &r.tree)
                    }
                    _ => false,
                };
                if input.definition.module == definition.module
                    && definition.member.is_none()
                    && left.declaration == right.declaration
                    && left.identity == right.identity
                    && left.closure_sites == right.closure_sites
                    && left.retained_members == right.retained_members
                    && generated
                    && selected.replace(input.item).is_some()
                {
                    return Err(error(
                        "E-SOURCE-IDENTITY",
                        actual.span,
                        "CT namespace input placement is ambiguous",
                    ));
                }
            }
            let input = selected.ok_or_else(|| {
                error(
                    "E-SOURCE-IDENTITY",
                    actual.span,
                    "CT namespace input has no actual source correspondence",
                )
            })?;
            struct CopyMeter<'a, 'b>(&'a mut Budget<'b>);
            impl crate::ct_selection::Visitor for CopyMeter<'_, '_> {
                fn node(&mut self, span: Span, depth: usize) -> CompileResult<()> {
                    self.0.node(span, depth).map_err(|e| vec![e])?;
                    self.0
                        .lineage_bytes(
                            std::mem::size_of::<Item>()
                                + std::mem::size_of::<Stmt>()
                                + std::mem::size_of::<Expr>()
                                + std::mem::size_of::<TypeRef>()
                                + std::mem::size_of::<Pattern>(),
                            span,
                        )
                        .map_err(|e| vec![e])
                }
                fn text(&mut self, text: &str, span: Span) -> CompileResult<()> {
                    self.0.text(text, span).map_err(|e| vec![e])?;
                    self.0.lineage_bytes(text.len(), span).map_err(|e| vec![e])
                }
            }
            crate::ct_selection::Visitor::item(&mut CopyMeter(&mut budget), input, 0)
                .map_err(|mut errors| errors.remove(0))?;
            let mut expected = input.clone();
            let mut visitor = Visitor {
                default_nominal: None,
                const_scope: Names::new(),
                mode: Mode::Normalize(self.index),
                module: definition.module,
                budget: &mut budget,
            };
            visitor.item(&mut expected, &Names::new(), true, 0)?;
            budget.item(&expected)?;
            budget.item(actual)?;
            if expected != *actual {
                return Err(Diagnostic::unsupported(
                    Phase::Expand,
                    "CT namespace whole input cannot be reproduced by its actual normalizer",
                    Some(actual.span),
                ));
            }
            Ok(input)
        })();
        *remaining = budget.remaining_limits();
        let input = result.map_err(|error| vec![error])?;
        self.callback
            .check_static_host(definition, input, remaining)
    }
    fn definition_expression(
        &self,
        definition: &crate::runtime_ct::RuntimeDefinition,
        ordinal: u32,
        remaining: &mut Limits,
    ) -> CompileResult<TypeRef> {
        self.callback
            .definition_expression(definition, ordinal, remaining)
    }
    fn instantiated(
        &self,
        instance: &crate::runtime_ct::RuntimeInstance,
        ledger: &[crate::runtime_ct::RuntimeInstance],
        template: &Item,
        output: &mut Item,
        remaining: &mut Limits,
    ) -> CompileResult<()> {
        self.callback
            .instantiated(instance, ledger, template, output, remaining)?;
        let mut budget = Budget::new(remaining);
        let mut visitor = Visitor {
            default_nominal: None,
            const_scope: Names::new(),
            mode: Mode::Normalize(self.index),
            module: instance.definition.module,
            budget: &mut budget,
        };
        visitor
            .item(output, &Names::new(), true, 0)
            .map_err(|e| vec![e])?;
        *remaining = budget.remaining_limits();
        Ok(())
    }
}

enum Mode<'a> {
    Normalize(&'a Index),
    Binders {
        bindings: BTreeMap<String, String>,
        occupied: Names,
        next: usize,
    },
    Restore {
        functions: &'a Functions,
        edges: &'a mut Vec<CoreImport>,
    },
}
struct Visitor<'a, 'b, 'l> {
    default_nominal: Option<String>,
    const_scope: Names,
    mode: Mode<'a>,
    module: ModuleId,
    budget: &'b mut Budget<'l>,
}
impl Visitor<'_, '_, '_> {
    fn names(&mut self, input: &Names, span: Span) -> R<Names> {
        for name in input {
            self.budget.node(span, 0)?;
            self.budget.text(name, span)?;
        }
        Ok(input.clone())
    }
    fn symbol<S: AsRef<str>>(
        &mut self,
        path: &[S],
        types: &Names,
        values: &Names,
        span: Span,
        type_position: bool,
    ) -> R<Option<String>> {
        if let Mode::Binders { bindings, .. } = &self.mode {
            let Some(name) = path.first() else {
                return Ok(None);
            };
            let name = name.as_ref();
            if type_position
                && types.contains(name)
                && let Some(canonical) = bindings.get(name)
            {
                self.budget.text(canonical, span)?;
                return Ok(Some(canonical.clone()));
            }
            return Ok(None);
        }
        let Mode::Normalize(index) = &self.mode else {
            return Ok(None);
        };
        let Some(first) = path.first().map(AsRef::as_ref) else {
            return Ok(None);
        };
        if types.contains(first)
            || (!type_position && (values.contains(first) || self.const_scope.contains(first)))
        {
            return Ok(None);
        }
        let (owner, name, external) = match path {
            [name] => (self.module, name.as_ref(), false),
            [alias, name] => {
                let Some(owner) = index.imports[&self.module].get(alias.as_ref()) else {
                    return Ok(None);
                };
                (*owner, name.as_ref(), true)
            }
            _ => return Ok(None),
        };
        let Some(symbol) = index.symbols.get(&owner).and_then(|s| s.get(name)) else {
            if external && type_position {
                return Err(error(
                    "E-MODULE-MEMBER",
                    span,
                    "imported module has no such type declaration",
                ));
            }
            return Ok(None);
        };
        if external && !symbol.public {
            // Ordinary function calls retain the established CT/types diagnostic
            // path. A private template or type cannot enter specialization.
            if symbol.function && !symbol.generic && !type_position {
                return Ok(None);
            }
            return Err(error(
                "E-MODULE-PRIVATE",
                span,
                "imported declaration is private",
            ));
        }
        self.budget.text(&symbol.canonical, span)?;
        Ok(Some(symbol.canonical.clone()))
    }
    fn binder_namespace(
        &mut self,
        expression: &mut Expr,
        types: &Names,
        values: &Names,
        depth: usize,
    ) -> R<()> {
        if !matches!(self.mode, Mode::Binders { .. }) {
            return Ok(());
        }
        self.budget.node(expression.span, depth)?;
        match &mut expression.kind {
            ExprKind::Name(name) if !values.contains(name) => {
                if let Some(canonical) = self.symbol(
                    std::slice::from_ref(name),
                    types,
                    values,
                    expression.span,
                    true,
                )? {
                    *name = canonical;
                }
            }
            ExprKind::Group(inner) | ExprKind::TypeApply { callee: inner, .. } => {
                self.binder_namespace(inner, types, values, depth + 1)?;
            }
            _ => {}
        }
        Ok(())
    }
    fn declaration_name(&mut self, name: &mut String, top: bool, span: Span) -> R<()> {
        if !top {
            return Ok(());
        }
        match &self.mode {
            Mode::Binders { .. } => {}
            Mode::Normalize(index) => {
                if let Some(symbol) = index.symbols[&self.module].get(name) {
                    self.budget.text(&symbol.canonical, span)?;
                    *name = symbol.canonical.clone();
                }
            }
            Mode::Restore { functions, .. } => {
                if let Some((_, original, _)) = functions.get(name) {
                    self.budget.text(original, span)?;
                    *name = original.clone();
                }
            }
        }
        Ok(())
    }
    fn bound(&mut self, ty: &mut TypeRef, types: &Names, depth: usize) -> R<()> {
        if crate::generics::structural_value_bound(ty) {
            return self.budget.ty(ty);
        }
        self.ty(ty, types, depth)
    }
    fn ty(&mut self, ty: &mut TypeRef, types: &Names, depth: usize) -> R<()> {
        self.budget.node(ty.span, depth)?;
        if let TypeKind::Dyn { bindings, .. } = &ty.kind
            && !bindings.is_empty()
        {
            return Err(Diagnostic::unsupported(
                Phase::Expand,
                "dyn binding module consumer is not yet implemented",
                Some(ty.span),
            ));
        }
        match &mut ty.kind {
            TypeKind::Named { path, args } | TypeKind::Dyn { path, args, .. } => {
                if let Some(name) = self.symbol(path, types, &Names::new(), ty.span, true)? {
                    if matches!(self.mode, Mode::Binders { .. }) {
                        path[0] = name;
                    } else {
                        *path = vec![name];
                    }
                } else if path.len() >= 2 {
                    // `Host.Member` and `module.Host.Member` carry the actual
                    // nominal host identity through associated projection.
                    let count = path.len();
                    if let Some(host) =
                        self.symbol(&path[..count - 1], types, &Names::new(), ty.span, true)?
                    {
                        *path = vec![host, path[count - 1].clone()];
                    }
                }
                for (position, arg) in args.iter_mut().enumerate() {
                    self.const_arg(
                        arg,
                        path.first().map(String::as_str),
                        position,
                        types,
                        &Names::new(),
                        depth + 1,
                    )?;
                }
            }
            TypeKind::Reference { inner, .. }
            | TypeKind::Group(inner)
            | TypeKind::Optional(inner)
            | TypeKind::ForLifetime { inner, .. } => self.ty(inner, types, depth + 1)?,
            TypeKind::Tuple(ts) | TypeKind::Union(ts) | TypeKind::Impl(ts) => {
                for t in ts {
                    self.ty(t, types, depth + 1)?;
                }
            }
            TypeKind::Array { element, length } => {
                self.ty(element, types, depth + 1)?;
                if let Some(e) = length {
                    self.expr(e, types, &Names::new(), depth + 1)?;
                }
            }
            TypeKind::Function { params, result }
            | TypeKind::ForeignFunction { params, result, .. } => {
                for t in params {
                    self.ty(t, types, depth + 1)?;
                }
                self.ty(result, types, depth + 1)?;
            }
            TypeKind::Error { error, result } => {
                self.ty(error, types, depth + 1)?;
                self.ty(result, types, depth + 1)?;
            }
            TypeKind::MetaEval(e) | TypeKind::MetaSplice(e) => {
                self.expr(e, types, &Names::new(), depth + 1)?
            }
            TypeKind::MetaBlock(b) => self.block(b, types, &Names::new(), depth + 1)?,
        }
        Ok(())
    }
    fn arg(&mut self, arg: &mut GenericArg, types: &Names, values: &Names, depth: usize) -> R<()> {
        match arg {
            GenericArg::Lifetime(name) => self.budget.text(name, Span::default()),
            GenericArg::Type(t) => self.ty(t, types, depth),
            GenericArg::Const(e) => self.expr(e, types, values, depth),
        }
    }
    fn const_arg(
        &mut self,
        argument: &mut GenericArg,
        target: Option<&str>,
        position: usize,
        types: &Names,
        values: &Names,
        depth: usize,
    ) -> R<()> {
        let is_const = if let (Mode::Normalize(index), Some(target)) = (&self.mode, target) {
            if index.const_parameters.is_empty() {
                false
            } else {
                let span = match argument {
                    GenericArg::Lifetime(_) => Span::default(),
                    GenericArg::Type(t) => t.span,
                    GenericArg::Const(e) => e.span,
                };
                self.budget.node(span, depth)?;
                self.budget.text(target, span)?;
                index
                    .const_parameters
                    .get(target)
                    .and_then(|slots| slots.get(position))
                    .copied()
                    .unwrap_or(false)
            }
        } else {
            false
        };
        if is_const
            && let GenericArg::Type(ty) = argument
            && let TypeKind::Named { path, args } = &ty.kind
            && let [name] = path.as_slice()
            && args.is_empty()
        {
            self.budget.node(ty.span, depth)?;
            self.budget.text(name, ty.span)?;
            *argument = GenericArg::Const(Expr {
                kind: ExprKind::Name(name.clone()),
                span: ty.span,
            });
        }
        self.arg(argument, types, values, depth)
    }
    fn item(&mut self, item: &mut Item, outer: &Names, top: bool, depth: usize) -> R<()> {
        self.budget.node(item.span, depth)?;
        let mut types = self.names(outer, item.span)?;
        let params = match &item.kind {
            ItemKind::Function(f) | ItemKind::Extern { function: f, .. } => f.generics.as_slice(),
            ItemKind::Aggregate(a) => a.generics.as_slice(),
            ItemKind::Enum(e) => e.generics.as_slice(),
            ItemKind::TypeAlias(a) => a.generics.as_slice(),
            ItemKind::Impl(i) => i.generics.as_slice(),
            ItemKind::Trait(t) => t.generics.as_slice(),
            _ => &[],
        };
        for name in &self.const_scope {
            self.budget.node(item.span, depth)?;
            self.budget.text(name, item.span)?;
        }
        let mut const_scope = self.const_scope.clone();
        for p in params {
            if matches!(p.kind, GenericParamKind::Type(_))
                && matches!(p.name.as_str(), "Value" | "SyncDrop")
            {
                return Err(Diagnostic::error(
                    Phase::Expand,
                    "E-GENERIC-ARGUMENT",
                    Some(p.span),
                    "Value and SyncDrop are reserved in type-binder and bound positions",
                ));
            }
            if matches!(p.kind, GenericParamKind::Type(_)) {
                types.insert(p.name.clone());
            } else if matches!(p.kind, GenericParamKind::Const(_)) {
                self.budget.node(p.span, depth)?;
                self.budget.text(&p.name, p.span)?;
                const_scope.insert(p.name.clone());
            }
        }
        let previous_consts = std::mem::replace(&mut self.const_scope, const_scope);
        let previous = if let Mode::Binders {
            bindings,
            occupied,
            next,
        } = &mut self.mode
        {
            for (source, canonical) in bindings.iter() {
                self.budget.node(item.span, depth)?;
                self.budget.text(source, item.span)?;
                self.budget.text(canonical, item.span)?;
            }
            let previous = bindings.clone();
            let mut seen = BTreeSet::new();
            for p in params {
                if !matches!(p.kind, GenericParamKind::Type(_)) || !seen.insert(&p.name) {
                    continue;
                }
                self.budget.text(&p.name, p.span)?;
                let canonical = loop {
                    self.budget.node(p.span, depth + 1)?;
                    let name = format!("$alias_type{}_{}", self.module.0, *next);
                    *next = next.checked_add(1).ok_or_else(|| {
                        Diagnostic::limit(
                            Phase::Expand,
                            "alias binder identity budget exhausted",
                            Some(p.span),
                        )
                    })?;
                    self.budget.text(&name, p.span)?;
                    if !occupied.contains(&name) {
                        break name;
                    }
                };
                bindings.insert(p.name.clone(), canonical);
            }
            Some(previous)
        } else {
            None
        };
        match &mut item.kind {
            ItemKind::Function(f) | ItemKind::Extern { function: f, .. } => {
                self.declaration_name(&mut f.name, top, item.span)?;
                let mut values = Names::new();
                for parameter in &mut f.generics {
                    match &mut parameter.kind {
                        GenericParamKind::Type(bounds) => {
                            for bound in bounds {
                                self.bound(bound, &types, depth + 1)?;
                            }
                        }
                        GenericParamKind::Const(ty) => {
                            self.ty(ty, &types, depth + 1)?;
                            values.insert(parameter.name.clone());
                        }
                        GenericParamKind::Lifetime => {}
                    }
                }
                // A where bound has the same lexical/module identity as an
                // inline bound. Resolve it before combining modules; leaving
                // source spellings here would lose privacy and nominal identity.
                for clause in &mut f.where_clause {
                    self.budget.node(clause.span, depth + 1)?;
                    match &mut clause.subject {
                        BoundSubject::Type(subject) => self.ty(subject, &types, depth + 1)?,
                        BoundSubject::Lifetime(name) => self.budget.text(name, clause.span)?,
                    }
                    for bound in &mut clause.bounds {
                        self.bound(bound, &types, depth + 1)?;
                    }
                    if let Some(name) = &clause.lifetime_bound {
                        self.budget.text(name, clause.span)?;
                    }
                }
                for p in &mut f.params {
                    self.ty(&mut p.ty, &types, depth + 1)?;
                    values.insert(p.name.clone());
                }
                if let Some(t) = &mut f.return_type {
                    self.ty(t, &types, depth + 1)?;
                }
                match &mut f.body {
                    FunctionBody::Block(b) => self.block(b, &types, &values, depth + 1)?,
                    FunctionBody::Expression(e) => self.expr(e, &types, &values, depth + 1)?,
                    FunctionBody::Declaration => {}
                }
            }
            ItemKind::Aggregate(a) => {
                self.declaration_name(&mut a.name, top, item.span)?;
                for parameter in &mut a.generics {
                    if let GenericParamKind::Type(bounds) = &mut parameter.kind {
                        for bound in bounds {
                            self.bound(bound, &types, depth + 1)?;
                        }
                    }
                }
                let previous_nominal = self.default_nominal.take();
                if a.fields.iter().any(|field| field.default.is_some()) {
                    self.budget.text(&a.name, item.span)?;
                    self.default_nominal = Some(a.name.clone());
                }
                for f in &mut a.fields {
                    self.ty(&mut f.ty, &types, depth + 1)?;
                    if let Some(e) = &mut f.default {
                        self.expr(e, &types, &Names::new(), depth + 1)?;
                    }
                }
                self.default_nominal = previous_nominal;
                for m in &mut a.methods {
                    self.item(m, &types, false, depth + 1)?;
                }
            }
            ItemKind::Enum(e) => {
                self.declaration_name(&mut e.name, top, item.span)?;
                for parameter in &mut e.generics {
                    if let GenericParamKind::Type(bounds) = &mut parameter.kind {
                        for bound in bounds {
                            self.bound(bound, &types, depth + 1)?;
                        }
                    }
                }
                for v in &mut e.variants {
                    match &mut v.fields {
                        VariantFields::Tuple(ts) => {
                            for t in ts {
                                self.ty(t, &types, depth + 1)?;
                            }
                        }
                        VariantFields::Record(fs) => {
                            for f in fs {
                                self.ty(&mut f.ty, &types, depth + 1)?;
                            }
                        }
                        VariantFields::Unit => {}
                    }
                }
            }
            ItemKind::TypeAlias(a) => {
                self.declaration_name(&mut a.name, top, item.span)?;
                for parameter in &mut a.generics {
                    if let GenericParamKind::Type(bounds) = &mut parameter.kind {
                        for bound in bounds {
                            self.bound(bound, &types, depth + 1)?;
                        }
                    }
                }
                for bound in &mut a.bounds {
                    self.bound(bound, &types, depth + 1)?;
                }
                if let Some(t) = &mut a.value {
                    self.ty(t, &types, depth + 1)?;
                }
            }
            ItemKind::Const(c) if !top => {
                if let Some(ty) = &mut c.ty {
                    self.ty(ty, &types, depth + 1)?;
                }
                if let Some(value) = &mut c.value {
                    self.expr(value, &types, &Names::new(), depth + 1)?;
                }
            }
            ItemKind::Impl(i) => {
                for parameter in &mut i.generics {
                    match &mut parameter.kind {
                        GenericParamKind::Type(bounds) => {
                            for bound in bounds {
                                self.bound(bound, &types, depth + 1)?;
                            }
                        }
                        GenericParamKind::Const(ty) => self.ty(ty, &types, depth + 1)?,
                        GenericParamKind::Lifetime => {}
                    }
                }
                for clause in &mut i.where_clause {
                    self.budget.node(clause.span, depth + 1)?;
                    match &mut clause.subject {
                        BoundSubject::Type(subject) => self.ty(subject, &types, depth + 1)?,
                        BoundSubject::Lifetime(name) => self.budget.text(name, clause.span)?,
                    }
                    for bound in &mut clause.bounds {
                        self.bound(bound, &types, depth + 1)?;
                    }
                    if let Some(name) = &clause.lifetime_bound {
                        self.budget.text(name, clause.span)?;
                    }
                }
                self.ty(&mut i.target, &types, depth + 1)?;
                if let Some(t) = &mut i.trait_type {
                    self.ty(t, &types, depth + 1)?;
                }
                if let Some(p) = &mut i.receiver_default {
                    self.ty(&mut p.ty, &types, depth + 1)?;
                }
                for m in &mut i.members {
                    self.item(m, &types, false, depth + 1)?;
                }
            }
            ItemKind::Trait(t) => {
                self.declaration_name(&mut t.name, top, item.span)?;
                for parameter in &mut t.generics {
                    if let GenericParamKind::Type(bounds) = &mut parameter.kind {
                        for bound in bounds {
                            self.bound(bound, &types, depth + 1)?;
                        }
                    }
                }
                for bound in &mut t.bounds {
                    self.bound(bound, &types, depth + 1)?;
                }
                for m in &mut t.members {
                    self.item(m, &types, false, depth + 1)?;
                }
            }
            _ => {}
        }
        if let (Some(previous), Mode::Binders { bindings, .. }) = (previous, &mut self.mode) {
            let params = match &mut item.kind {
                ItemKind::Function(f) | ItemKind::Extern { function: f, .. } => &mut f.generics,
                ItemKind::Aggregate(a) => &mut a.generics,
                ItemKind::Enum(e) => &mut e.generics,
                ItemKind::TypeAlias(a) => &mut a.generics,
                ItemKind::Impl(i) => &mut i.generics,
                ItemKind::Trait(t) => &mut t.generics,
                _ => {
                    *bindings = previous;
                    self.const_scope = previous_consts;
                    return Ok(());
                }
            };
            for p in params {
                if matches!(p.kind, GenericParamKind::Type(_)) {
                    let canonical = &bindings[&p.name];
                    self.budget.text(canonical, p.span)?;
                    p.name = canonical.clone();
                }
            }
            *bindings = previous;
        }
        self.const_scope = previous_consts;
        Ok(())
    }
    fn pattern(
        &mut self,
        p: &mut Pattern,
        types: &Names,
        values: &mut Names,
        depth: usize,
    ) -> R<()> {
        self.budget.node(p.span, depth)?;
        match &mut p.kind {
            PatternKind::Name(n) => {
                values.insert(n.clone());
            }
            PatternKind::Tuple(ps) => {
                for p in ps {
                    self.pattern(p, types, values, depth + 1)?;
                }
            }
            PatternKind::Reference { pattern, .. } | PatternKind::ErrorArm { pattern, .. } => {
                self.pattern(pattern, types, values, depth + 1)?
            }
            PatternKind::Constructor { path, arguments } => {
                if let Some(name) = self.symbol(path, types, &Names::new(), p.span, true)? {
                    *path = vec![name];
                } else if path.len() >= 2 {
                    let count = path.len();
                    if let Some(name) =
                        self.symbol(&path[..count - 1], types, &Names::new(), p.span, true)?
                    {
                        *path = vec![name, path[count - 1].clone()];
                    }
                }
                for p in arguments {
                    self.pattern(p, types, values, depth + 1)?;
                }
            }
            PatternKind::RecordConstructor { path, fields } => {
                if let Some(name) = self.symbol(path, types, &Names::new(), p.span, true)? {
                    *path = vec![name];
                } else if path.len() >= 2 {
                    let count = path.len();
                    if let Some(name) =
                        self.symbol(&path[..count - 1], types, &Names::new(), p.span, true)?
                    {
                        *path = vec![name, path[count - 1].clone()];
                    }
                }
                for field in fields {
                    self.budget.node(field.span, depth + 1)?;
                    self.budget.text(&field.name, field.span)?;
                    self.pattern(&mut field.pattern, types, values, depth + 1)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    fn block(&mut self, b: &mut Block, types: &Names, values: &Names, depth: usize) -> R<()> {
        self.budget.node(b.span, depth)?;
        let mut scope = self.names(values, b.span)?;
        for s in &mut b.statements {
            self.statement(s, types, &mut scope, depth + 1)?;
        }
        Ok(())
    }
    fn condition(
        &mut self,
        c: &mut Condition,
        types: &Names,
        values: &mut Names,
        depth: usize,
    ) -> R<()> {
        match c {
            Condition::Expression(e) => self.expr(e, types, values, depth),
            Condition::Let { pattern, value } => {
                self.expr(value, types, values, depth)?;
                self.pattern(pattern, types, values, depth)
            }
        }
    }
    fn statement(
        &mut self,
        s: &mut Stmt,
        types: &Names,
        values: &mut Names,
        depth: usize,
    ) -> R<()> {
        self.budget.node(s.span, depth)?;
        match &mut s.kind {
            StmtKind::Binding {
                pattern, ty, value, ..
            } => {
                if let Some(t) = ty {
                    self.ty(t, types, depth + 1)?;
                }
                self.expr(value, types, values, depth + 1)?;
                self.pattern(pattern, types, values, depth + 1)?;
            }
            StmtKind::Return(Some(e)) | StmtKind::Yield(e) | StmtKind::Expression(e) => {
                self.expr(e, types, values, depth + 1)?
            }
            StmtKind::Assign { target, value, .. } => {
                self.expr(target, types, values, depth + 1)?;
                self.expr(value, types, values, depth + 1)?;
            }
            StmtKind::If {
                condition,
                then_body,
                else_body,
            } => {
                let mut success = self.names(values, s.span)?;
                self.condition(condition, types, &mut success, depth + 1)?;
                self.block(then_body, types, &success, depth + 1)?;
                match else_body {
                    Some(ElseBranch::Block(b)) => self.block(b, types, values, depth + 1)?,
                    Some(ElseBranch::If(s)) => {
                        let mut scope = self.names(values, s.span)?;
                        self.statement(s, types, &mut scope, depth + 1)?;
                    }
                    None => {}
                }
            }
            StmtKind::While { condition, body } => {
                let mut success = self.names(values, s.span)?;
                self.condition(condition, types, &mut success, depth + 1)?;
                self.block(body, types, &success, depth + 1)?;
            }
            StmtKind::For {
                pattern,
                iterable,
                body,
            } => {
                self.expr(iterable, types, values, depth + 1)?;
                let mut scope = self.names(values, s.span)?;
                self.pattern(pattern, types, &mut scope, depth + 1)?;
                self.block(body, types, &scope, depth + 1)?;
            }
            StmtKind::Loop(b) | StmtKind::Defer(b) | StmtKind::Unsafe(b) => {
                self.block(b, types, values, depth + 1)?
            }
            StmtKind::With { name, value, body } => {
                self.expr(value, types, values, depth + 1)?;
                let mut scope = self.names(values, s.span)?;
                scope.insert(name.clone());
                self.block(body, types, &scope, depth + 1)?;
            }
            StmtKind::Match { value, arms } => {
                self.expr(value, types, values, depth + 1)?;
                for arm in arms {
                    let mut scope = self.names(values, arm.span)?;
                    self.pattern(&mut arm.pattern, types, &mut scope, depth + 1)?;
                    self.arrow(&mut arm.body, types, &scope, depth + 1)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    fn arrow(&mut self, b: &mut ArrowBody, types: &Names, values: &Names, depth: usize) -> R<()> {
        match b {
            ArrowBody::Expression(e) => self.expr(e, types, values, depth),
            ArrowBody::Block(b) => self.block(b, types, values, depth),
        }
    }
    fn expr(&mut self, e: &mut Expr, types: &Names, values: &Names, depth: usize) -> R<()> {
        self.budget.node(e.span, depth)?;
        if self.expr_name(e, types, values, depth)? {
            return Ok(());
        }
        self.expr_children(e, types, values, depth)
    }

    // Syntax reconstruction has no recursive expression edge. Keep its owned
    // temporary storage out of every recursive child's native stack frame.
    #[inline(never)]
    fn expr_name(&mut self, e: &mut Expr, types: &Names, values: &Names, depth: usize) -> R<bool> {
        // Self in a default denotes its defining nominal, unless a lexical
        // value binder shadows that expression name. Reuse this visitor's
        // existing scoped name sets; do not rewrite arbitrary Self.x values.
        if matches!(&e.kind, ExprKind::Name(name) if name == "Self")
            && !values.contains("Self")
            && let Some(nominal) = &self.default_nominal
        {
            self.budget.text(nominal, e.span)?;
            e.kind = ExprKind::Name(nominal.clone());
            return Ok(true);
        }
        // Lookup paths are transient borrowed names, not new syntax. Restore
        // cannot normalize a symbol; its separate canonical-name lookup below
        // retains all of its original work, charges and generated imports.
        let normalized = if matches!(self.mode, Mode::Restore { .. }) {
            None
        } else {
            match &e.kind {
                ExprKind::Name(name) => {
                    self.symbol(std::slice::from_ref(name), types, values, e.span, false)?
                }
                ExprKind::Member {
                    object,
                    member: MemberName::Name(name),
                } => match &object.kind {
                    ExprKind::Name(alias) => self.symbol(
                        &[alias.as_str(), name.as_str()],
                        types,
                        values,
                        e.span,
                        false,
                    )?,
                    _ => None,
                },
                _ => None,
            }
        };
        if let Some(name) = normalized {
            e.kind = ExprKind::Name(name);
            return Ok(true);
        }
        if let (Mode::Restore { functions, edges }, ExprKind::Name(name)) =
            (&mut self.mode, &e.kind)
            && let Some((owner, local, _)) = functions.get(name)
        {
            self.budget.text(local, e.span)?;
            e.kind = if *owner == self.module {
                ExprKind::Name(local.clone())
            } else {
                let alias = if let Some(edge) = edges.iter().find(|edge| edge.module == *owner) {
                    edge.alias.clone()
                } else {
                    let alias = format!("$module_import_{}", owner.0);
                    self.budget.node(e.span, depth)?;
                    self.budget.text(&alias, e.span)?;
                    edges.push(CoreImport {
                        alias: alias.clone(),
                        module: *owner,
                        span: e.span,
                    });
                    alias
                };
                ExprKind::Member {
                    object: Box::new(Expr {
                        kind: ExprKind::Name(alias),
                        span: e.span,
                    }),
                    member: MemberName::Name(local.clone()),
                }
            };
            return Ok(true);
        }
        Ok(false)
    }

    #[inline(never)]
    fn expr_children(
        &mut self,
        e: &mut Expr,
        types: &Names,
        values: &Names,
        depth: usize,
    ) -> R<()> {
        match &mut e.kind {
            ExprKind::Ascribed { .. } => self.expr_ascribed(e, types, values, depth),
            ExprKind::Group(x)
            | ExprKind::Try(x)
            | ExprKind::MetaEval(x)
            | ExprKind::Unary { operand: x, .. } => self.expr(x, types, values, depth + 1),
            ExprKind::Tuple(xs) | ExprKind::Array(xs) => {
                self.expr_list(xs, types, values, depth + 1)
            }
            ExprKind::Binary { left, right, .. } => {
                self.expr_pair(left, right, types, values, depth + 1)
            }
            ExprKind::Pipeline { input, target } => {
                self.expr_pair(input, target, types, values, depth + 1)
            }
            ExprKind::Member { .. } => self.expr_member(e, types, values, depth),
            ExprKind::TypeApply { .. } => self.expr_type_apply(e, types, values, depth),
            ExprKind::Call { .. } => self.expr_call(e, types, values, depth),
            ExprKind::Index { object, index } => {
                self.expr_pair(object, index, types, values, depth + 1)
            }
            ExprKind::Slice { .. } => self.expr_slice(e, types, values, depth),
            ExprKind::Record { .. } => self.expr_record(e, types, values, depth),
            ExprKind::Closure { .. } => self.expr_closure(e, types, values, depth),
            ExprKind::If { .. } => self.expr_if(e, types, values, depth),
            ExprKind::MetaBlock(b) => self.block(b, types, values, depth + 1),
            _ => Ok(()),
        }
    }

    #[inline(never)]
    fn expr_pair(
        &mut self,
        first: &mut Expr,
        second: &mut Expr,
        types: &Names,
        values: &Names,
        depth: usize,
    ) -> R<()> {
        self.expr(first, types, values, depth)?;
        self.expr(second, types, values, depth)
    }

    #[inline(never)]
    fn expr_list(
        &mut self,
        expressions: &mut [Expr],
        types: &Names,
        values: &Names,
        depth: usize,
    ) -> R<()> {
        for expression in expressions {
            self.expr(expression, types, values, depth)?;
        }
        Ok(())
    }

    #[inline(never)]
    fn expr_ascribed(
        &mut self,
        e: &mut Expr,
        types: &Names,
        values: &Names,
        depth: usize,
    ) -> R<()> {
        let ExprKind::Ascribed { ty, value } = &mut e.kind else {
            unreachable!("Ascribed dispatcher")
        };
        self.ty(ty, types, depth + 1)?;
        self.expr(value, types, values, depth + 1)
    }

    #[inline(never)]
    fn expr_member(&mut self, e: &mut Expr, types: &Names, values: &Names, depth: usize) -> R<()> {
        let ExprKind::Member { object, member } = &mut e.kind else {
            unreachable!("Member dispatcher")
        };
        if let MemberName::Constrained { owner, .. } = member {
            self.ty(owner, types, depth + 1)?;
        }
        self.binder_namespace(object, types, values, depth + 1)?;
        self.expr(object, types, values, depth + 1)
    }

    #[inline(never)]
    fn expr_call(&mut self, e: &mut Expr, types: &Names, values: &Names, depth: usize) -> R<()> {
        let ExprKind::Call { callee, arguments } = &mut e.kind else {
            unreachable!("Call dispatcher")
        };
        self.expr(callee, types, values, depth + 1)?;
        for arg in arguments {
            self.expr(&mut arg.value, types, values, depth + 1)?;
        }
        Ok(())
    }

    #[inline(never)]
    fn expr_slice(&mut self, e: &mut Expr, types: &Names, values: &Names, depth: usize) -> R<()> {
        let ExprKind::Slice { object, start, end } = &mut e.kind else {
            unreachable!("Slice dispatcher")
        };
        self.expr(object, types, values, depth + 1)?;
        for x in [start, end].into_iter().flatten() {
            self.expr(x, types, values, depth + 1)?;
        }
        Ok(())
    }

    #[inline(never)]
    fn expr_if(&mut self, e: &mut Expr, types: &Names, values: &Names, depth: usize) -> R<()> {
        let ExprKind::If {
            condition,
            then_value,
            else_value,
        } = &mut e.kind
        else {
            unreachable!("If dispatcher")
        };
        self.expr(condition, types, values, depth + 1)?;
        self.expr(then_value, types, values, depth + 1)?;
        self.expr(else_value, types, values, depth + 1)
    }

    #[inline(never)]
    fn expr_type_apply(
        &mut self,
        e: &mut Expr,
        types: &Names,
        values: &Names,
        depth: usize,
    ) -> R<()> {
        let ExprKind::TypeApply { callee, arguments } = &mut e.kind else {
            unreachable!("TypeApply dispatcher")
        };

        self.expr(callee, types, values, depth + 1)?;
        for (position, arg) in arguments.iter_mut().enumerate() {
            let target = if let ExprKind::Name(name) = &callee.kind {
                Some(name.as_str())
            } else {
                None
            };
            self.const_arg(arg, target, position, types, values, depth + 1)?;
        }
        Ok(())
    }

    #[inline(never)]
    fn expr_record(&mut self, e: &mut Expr, types: &Names, values: &Names, depth: usize) -> R<()> {
        let ExprKind::Record { ty, base, fields } = &mut e.kind else {
            unreachable!("Record dispatcher")
        };

        if let Some(t) = ty {
            self.binder_namespace(t, types, values, depth + 1)?;
            self.expr(t, types, values, depth + 1)?;
        }
        if let Some(b) = base {
            self.expr(b, types, values, depth + 1)?;
        }
        for f in fields {
            if let Some(x) = &mut f.value {
                self.expr(x, types, values, depth + 1)?;
            } else if let Some(name) =
                self.symbol(std::slice::from_ref(&f.name), types, values, f.span, false)?
            {
                f.value = Some(Expr {
                    kind: ExprKind::Name(name),
                    span: f.span,
                });
            }
        }
        Ok(())
    }

    #[inline(never)]
    fn expr_closure(&mut self, e: &mut Expr, types: &Names, values: &Names, depth: usize) -> R<()> {
        let ExprKind::Closure {
            params,
            return_type,
            body,
            ..
        } = &mut e.kind
        else {
            unreachable!("Closure dispatcher")
        };

        let mut scope = self.names(values, e.span)?;
        for p in params {
            if let Some(t) = &mut p.ty {
                self.ty(t, types, depth + 1)?;
            }
            scope.insert(p.name.clone());
        }
        if let Some(t) = return_type {
            self.ty(t, types, depth + 1)?;
        }
        self.arrow(body, types, &scope, depth + 1)?;
        Ok(())
    }
}
