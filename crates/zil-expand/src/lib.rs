#![forbid(unsafe_code)]
//! Source-shaped AST to untrusted Core. No transformation grants safety facts.
//! Sequence bindings identify ordered evaluations. They do not decide a place's
//! Copy/Move/Borrow mode or extend a borrow; those decisions remain in zil-types.
pub mod associated_constants;
pub mod ct_selection;
mod dyn_lowering;
mod enums;
mod foreign;
mod generated_spans;
mod generics;
mod lifetimes;
mod record_enums;
pub use generated_spans::{remap_generated_spans,remap_generated_spans_metered};
mod mapping;
mod modules;
mod project_traits;
mod receivers;
mod records;
mod resources;
pub mod runtime_ct;
pub mod source_context;
mod static_traits;
mod stored_types;
mod tasks;

use std::collections::{BTreeMap, BTreeSet};
use zil_base::{
    CompileResult, Diagnostic, ExpansionRole, Limits, NodeId, Phase, SCHEMA_VERSION, SourceMap,
    SourceMapId, SourceTarget, SourceUnit, Span,
};
use zil_ir::*;
use zil_syntax::ast as s;

/// These names are source primitives, never ordinary function values.
fn integer_name(name: &str) -> bool {
    matches!(
        name,
        "u8" | "u16" | "u64" | "usize" | "i32" | "i64" | "f32" | "f64"
    )
}
fn foreign_constructor_name(name: &str) -> bool {
    matches!(
        name,
        "c_null" | "c_wstr_null" | "c_wstr" | "c_callback" | "c_with_callback"
    )
}
fn byte_buffer_name(name: &str) -> bool {
    matches!(
        name,
        "ByteBuffer"
            | "bytes_new"
            | "bytes_reserve"
            | "bytes_truncate"
            | "bytes_push"
            | "bytes_len"
            | "bytes_capacity"
            | "bytes_view"
            | "bytes_view_mut"
    )
}
fn owned_store_name(name: &str) -> bool {
    matches!(
        name,
        "Managed"
            | "Store"
            | "StoreElement"
            | "store_new"
            | "store_reserve"
            | "store_push"
            | "store_pop"
            | "store_len"
            | "store_capacity"
            | "store_at"
            | "store_at_mut"
    )
}
fn sync_name(name: &str) -> bool {
    matches!(
        name,
        "MutexI32"
            | "AtomicI32"
            | "mutex_scope"
            | "atomic_scope"
            | "mutex_load"
            | "mutex_store"
            | "mutex_fetch_add"
            | "atomic_load"
            | "atomic_store"
            | "atomic_fetch_add"
            | "atomic_compare_exchange"
    )
}
fn task_name(name: &str) -> bool {
    matches!(
        name,
        "TaskI32"
            | "Task"
            | "task_create"
            | "task_start"
            | "task_yield"
            | "task_await"
            | "task_valid"
            | "task_poll"
            | "task_request_cancel"
            | "task_poll_close"
            | "task_finish_destroy"
    )
}
fn registration_name(name: &str) -> bool {
    matches!(
        name,
        "Registration"
            | "AsyncRegistration"
            | "ClosedEnvironment"
            | "c_register"
            | "registration_valid"
            | "registration_close"
            | "c_register_async"
            | "async_registration_valid"
            | "async_registration_request_close"
            | "async_registration_poll_close"
            | "async_registration_finish_destroy"
    )
}
fn primitive_name(name: &str) -> bool {
    registration_name(name)
        || task_name(name)
        || sync_name(name)
        || name == "Option"
        || name == "thread_scope"
        || integer_name(name)
        || foreign_constructor_name(name)
        || byte_buffer_name(name)
        || owned_store_name(name)
}
fn primitive_name_error(name: &str, span: Span) -> Diagnostic {
    if task_name(name) {
        Diagnostic::error(
            Phase::Expand,
            "E-TASK-NAME",
            Some(span),
            "sealed task names cannot be rebound",
        )
    } else if sync_name(name) {
        Diagnostic::error(
            Phase::Expand,
            "E-SYNC-NAME",
            Some(span),
            "sealed synchronization names cannot be rebound",
        )
    } else if name == "Option" {
        Diagnostic::error(
            Phase::Expand,
            "E-OPTION-NAME",
            Some(span),
            "Option is a reserved built-in type and constructor namespace",
        )
    } else if name == "thread_scope" {
        Diagnostic::error(
            Phase::Expand,
            "E-THREAD-NAME",
            Some(span),
            "thread_scope is a reserved structured primitive and cannot be rebound",
        )
    } else if name == "c_with_callback" {
        Diagnostic::error(
            Phase::Expand,
            "E-FFI-SCOPED-NAME",
            Some(span),
            "c_with_callback is a reserved bare structured primitive and cannot be rebound",
        )
    } else if owned_store_name(name) {
        Diagnostic::error(
            Phase::Expand,
            "E-OWNED-STORE-NAME",
            Some(span),
            "Managed, Store and bare owned storage primitives cannot be rebound",
        )
    } else if byte_buffer_name(name) {
        Diagnostic::error(
            Phase::Expand,
            "E-BYTE-BUFFER-NAME",
            Some(span),
            "ByteBuffer and bare byte storage primitives cannot be rebound",
        )
    } else if integer_name(name) {
        conversion_name_error(span)
    } else {
        Diagnostic::error(
            Phase::Expand,
            "E-FFI-CONSTRUCTOR-NAME",
            Some(span),
            "bare static foreign constructor names cannot be rebound",
        )
    }
}
fn integer_conversion_name(expression: &s::Expr) -> Option<&str> {
    match &expression.kind {
        s::ExprKind::Name(name) if integer_name(name) => Some(name),
        s::ExprKind::Group(inner) => integer_conversion_name(inner),
        _ => None,
    }
}
fn conversion_error(span: Span, message: &str) -> Diagnostic {
    Diagnostic::error(Phase::Expand, "E-INTEGER-CONVERSION", Some(span), message)
}
fn conversion_name_error(span: Span) -> Diagnostic {
    Diagnostic::error(
        Phase::Expand,
        "E-INTEGER-CONVERSION-NAME",
        Some(span),
        "u8, u16, u64, usize, i32 and i64 are reserved bare integer conversion names",
    )
}

/// Preserve ordinary nominal/alias reserved-name checks before an untrusted
/// source generator alpha-renames a declaration. This grants no type facts.
pub fn check_generated_type_name(name: &str, span: Span) -> CompileResult<()> {
    generics::check_generated_type_name(name, span).map_err(|error| vec![error])
}

/// Preserve the ordinary function primitive-name rule before hygiene changes
/// its source spelling. Function and nominal reserved-name sets are distinct.
pub fn check_generated_function_name(name: &str, span: Span) -> CompileResult<()> {
    if primitive_name(name) {
        Err(vec![primitive_name_error(name, span)])
    } else {
        Ok(())
    }
}

mod callable_context_lower;
use callable_context_lower::CallableContextEmission;
mod closure_context_lower;
use closure_context_lower::ClosureContextEmission;

#[derive(Clone, Debug)]
pub struct ExpandedModule {
    pub core: CoreModule,
    pub source_map: SourceMap,
    // Compiler-generated ordinary imports, never safety capabilities.
    generated_imports: Vec<CoreImport>,
    generated_exports: Vec<NodeId>,
    imported_public_traits: BTreeSet<String>,
    trait_method_names: BTreeMap<(usize, usize), String>,
    dyn_method_emissions: Vec<(usize, usize, NodeId)>,
    closure_context_emissions: Vec<ClosureContextEmission>,
    callable_context_emissions: Vec<CallableContextEmission>,
    project_trait_work: usize,
    project_trait_nodes: usize,
    member_owner_work: usize,
    member_owner_nodes: usize,
}

type ProjectDefaults = BTreeMap<(String, String), (zil_base::ModuleId, String)>;

pub fn expand(source: &SourceUnit, module: &s::Module) -> CompileResult<CoreModule> {
    Ok(expand_with_map(source, module)?.core)
}

pub fn expand_with_map(source: &SourceUnit, module: &s::Module) -> CompileResult<ExpandedModule> {
    expand_with_map_limits(source, module, &Limits::default())
}

pub fn expand_with_map_limits(
    source: &SourceUnit,
    module: &s::Module,
    limits: &Limits,
) -> CompileResult<ExpandedModule> {
    expand_with_map_limits_target(source, module, limits, &zil_base::TargetProfile::default())
}

pub fn expand_with_map_limits_target(
    source: &SourceUnit,
    module: &s::Module,
    limits: &Limits,
    target: &zil_base::TargetProfile,
) -> CompileResult<ExpandedModule> {
    expand_with_map_limits_target_impl(source, module, limits, target, None)
}
/// Explicit declaration naming/lineage input; this view grants no permissions.
pub fn expand_with_map_limits_target_context(
    source: &SourceUnit,
    module: &s::Module,
    limits: &Limits,
    target: &zil_base::TargetProfile,
    context: source_context::SourceIdentityContext<'_>,
) -> CompileResult<ExpandedModule> {
    expand_with_map_limits_target_impl(source, module, limits, target, Some(context))
}
fn expand_with_map_limits_target_impl(
    source: &SourceUnit,
    module: &s::Module,
    limits: &Limits,
    target: &zil_base::TargetProfile,
    context: Option<source_context::SourceIdentityContext<'_>>,
) -> CompileResult<ExpandedModule> {
    let mut products = generics::products::Products::default();
    products.target_pointer_width = Some(target.pointer_width);
    let specialized = generics::specialize_products_with_context(
        module,
        limits,
        &mut products,
        context.map(|context| (std::slice::from_ref(source), context)),
    )?;
    let mut lower_limits = products.trait_limits(limits);
    let mut expanded = expand_inner(
        source,
        &specialized,
        false,
        &lower_limits,
        &products.trait_signatures,
        products.metered(),
        &products.member_patterns,
        &products.dynamic,
        &products.storage,
    )?;
    if products.metered() {
        debit_trait_lowering(&expanded, &mut lower_limits, 1)?;
        products.allowance = Some(lower_limits);
    }
    products.bind_dynamic_targets(&specialized, &expanded, limits)?;
    products.bind_closure_contexts(&specialized, &mut expanded, limits)?;
    products.bind_callable_contexts(&specialized, &mut expanded, limits)?;
    products.finish_closure_contexts(limits)?;
    products.finish_callable_contexts(limits)?;
    products.publish(
        &mut [(&mut expanded.core, &mut expanded.source_map)],
        std::slice::from_ref(source),
        limits,
    )?;
    Ok(expanded)
}

pub fn expand_program_module(
    source: &SourceUnit,
    module: &s::Module,
    imports: Vec<CoreImport>,
) -> CompileResult<(ProgramModule, SourceMap)> {
    expand_program_module_with_limits(source, module, imports, &Limits::default())
}

pub fn expand_program_module_with_limits(
    source: &SourceUnit,
    module: &s::Module,
    imports: Vec<CoreImport>,
    limits: &Limits,
) -> CompileResult<(ProgramModule, SourceMap)> {
    expand_program_module_with_limits_target(
        source,
        module,
        imports,
        limits,
        &zil_base::TargetProfile::default(),
    )
}

pub fn expand_program_module_with_limits_target(
    source: &SourceUnit,
    module: &s::Module,
    imports: Vec<CoreImport>,
    limits: &Limits,
    target: &zil_base::TargetProfile,
) -> CompileResult<(ProgramModule, SourceMap)> {
    let mut products = generics::products::Products::default();
    products.target_pointer_width = Some(target.pointer_width);
    let specialized = generics::specialize_products(module, limits, &mut products)?;
    let module = &specialized;
    let mut lower_limits = products.trait_limits(limits);
    let mut expanded = expand_inner(
        source,
        module,
        true,
        &lower_limits,
        &products.trait_signatures,
        products.metered(),
        &products.member_patterns,
        &products.dynamic,
        &products.storage,
    )?;
    if products.metered() {
        debit_trait_lowering(&expanded, &mut lower_limits, 1)?;
        products.allowance = Some(lower_limits);
    }
    products.bind_dynamic_targets(module, &expanded, limits)?;
    products.bind_closure_contexts(module, &mut expanded, limits)?;
    products.bind_callable_contexts(module, &mut expanded, limits)?;
    products.finish_closure_contexts(limits)?;
    products.finish_callable_contexts(limits)?;
    products.publish(
        &mut [(&mut expanded.core, &mut expanded.source_map)],
        std::slice::from_ref(source),
        limits,
    )?;
    let exports = source_exports(
        module,
        &expanded.core,
        &expanded.imported_public_traits,
        &expanded.trait_method_names,
    );
    Ok((
        ProgramModule {
            core: expanded.core,
            imports,
            exports,
        },
        expanded.source_map,
    ))
}

/// Export ordinary functions and declared public methods. A method's
/// visibility comes from its declaration and public nominal owner, never from
/// seeing a same-spelled method in a downstream module. Trait signatures in a
/// public local or checked imported trait are public through their public
/// concrete implementation.
fn source_exports(
    module: &s::Module,
    core: &CoreModule,
    imported_public_traits: &BTreeSet<String>,
    trait_method_names: &BTreeMap<(usize, usize), String>,
) -> Vec<NodeId> {
    let public_nominals: std::collections::BTreeSet<_> = module
        .items
        .iter()
        .filter_map(|item| {
            if !item.public {
                return None;
            }
            match &item.kind {
                s::ItemKind::Aggregate(a) => Some(a.name.as_str()),
                _ => None,
            }
        })
        .collect();
    let public_traits: std::collections::BTreeSet<_> = module
        .items
        .iter()
        .filter_map(|item| {
            if let s::ItemKind::Trait(t) = &item.kind
                && item.public
            {
                Some(t.name.as_str())
            } else {
                None
            }
        })
        .collect();
    let functions: std::collections::BTreeMap<_, _> = core
        .functions
        .iter()
        .map(|f| (f.name.clone(), f.id))
        .collect();
    let mut result = Vec::new();
    let mut add =
        |f: &s::Function, prefix: Option<&str>, implementation: Option<(usize, usize)>| {
            let target = implementation
                .and_then(|identity| trait_method_names.get(&identity))
                .unwrap_or(&f.name);
            let name = prefix.map_or_else(|| target.clone(), |owner| format!("{owner}.{target}"));
            if let Some(id) = functions.get(&name) {
                result.push(*id);
            }
        };
    for (item_ordinal, item) in module.items.iter().enumerate() {
        match &item.kind {
            s::ItemKind::Function(f) if item.public => add(f, None, None),
            s::ItemKind::Aggregate(a) if public_nominals.contains(a.name.as_str()) => {
                for member in &a.methods {
                    if member.public
                        && let s::ItemKind::Function(f) = &member.kind
                    {
                        add(f, Some(&a.name), None);
                    }
                }
            }
            s::ItemKind::Impl(i) => {
                let Some(owner) = type_path(&i.target) else {
                    continue;
                };
                if !public_nominals.contains(owner.as_str()) {
                    continue;
                }
                let public_trait = i
                    .trait_type
                    .as_ref()
                    .and_then(type_path)
                    .is_some_and(|name| {
                        public_traits.contains(name.as_str())
                            || imported_public_traits.contains(&name)
                    });
                for (member_ordinal, member) in i.members.iter().enumerate() {
                    if (if i.trait_type.is_some() {
                        public_trait
                    } else {
                        member.public
                    }) && let s::ItemKind::Function(f) = &member.kind
                    {
                        add(f, Some(&owner), Some((item_ordinal, member_ordinal)));
                    }
                }
            }
            _ => {}
        }
    }
    result
}

/// Specialize a resolved project as one bounded syntax input, then expand each
/// declaration in its original module. The import graph contains source-level
/// identities, while hygienic names only disambiguate syntax during this pass.
pub fn expand_program_with_limits(
    sources: &[SourceUnit],
    syntax: &[&s::Module],
    imports: &[Vec<CoreImport>],
    limits: &Limits,
) -> CompileResult<Vec<(ProgramModule, SourceMap)>> {
    expand_program_with_limits_target(
        sources,
        syntax,
        imports,
        limits,
        &zil_base::TargetProfile::default(),
    )
}

pub fn expand_program_with_limits_target(
    sources: &[SourceUnit],
    syntax: &[&s::Module],
    imports: &[Vec<CoreImport>],
    limits: &Limits,
    target: &zil_base::TargetProfile,
) -> CompileResult<Vec<(ProgramModule, SourceMap)>> {
    expand_program_with_limits_target_impl(sources, syntax, imports, limits, target, None, None)
}
/// The snapshot mode requires the complete original closed request. Explicit
/// mode additionally supplies canonical logical module identities.
pub fn expand_program_with_limits_target_context(
    sources: &[SourceUnit],
    syntax: &[&s::Module],
    imports: &[Vec<CoreImport>],
    limits: &Limits,
    target: &zil_base::TargetProfile,
    context: source_context::SourceIdentityContext<'_>,
) -> CompileResult<Vec<(ProgramModule, SourceMap)>> {
    expand_program_with_limits_target_impl(
        sources,
        syntax,
        imports,
        limits,
        target,
        Some(context),
        None,
    )
}
/// Expand with a source-authenticated CT consumer at the runtime instantiation
/// boundary. Ordinary callers preserve the existing specialization path.
pub fn expand_program_with_runtime_ct(
    sources: &[SourceUnit],
    syntax: &[&s::Module],
    imports: &[Vec<CoreImport>],
    limits: &Limits,
    target: &zil_base::TargetProfile,
    context: source_context::SourceIdentityContext<'_>,
    runtime_ct: &dyn runtime_ct::RuntimeCt,
) -> CompileResult<Vec<(ProgramModule, SourceMap)>> {
    expand_program_with_limits_target_impl(
        sources,
        syntax,
        imports,
        limits,
        target,
        Some(context),
        Some(runtime_ct),
    )
}
fn expand_program_with_limits_target_impl(
    sources: &[SourceUnit],
    syntax: &[&s::Module],
    imports: &[Vec<CoreImport>],
    limits: &Limits,
    target: &zil_base::TargetProfile,
    context: Option<source_context::SourceIdentityContext<'_>>,
    runtime_ct: Option<&dyn runtime_ct::RuntimeCt>,
) -> CompileResult<Vec<(ProgramModule, SourceMap)>> {
    let mut products = generics::products::Products::default();
    products.target_pointer_width = Some(target.pointer_width);
    let specialized = if let Some(runtime_ct) = runtime_ct {
        modules::specialize_with_runtime(
            sources,
            syntax,
            imports,
            limits,
            &mut products,
            context,
            Some(runtime_ct),
        )?
    } else {
        modules::specialize_with_context(sources, syntax, imports, limits, &mut products, context)?
    };
    let project_modules: Vec<_> = specialized.iter().map(|(module, _)| module).collect();
    let declarations = declaration_index(project_modules.iter().copied());
    let mut result = Vec::new();
    let mut remaining = products.trait_limits(limits);
    let mut defaults = ProjectDefaults::new();
    // Defaults execute in their declaration's module. The generated function
    // name cannot be spelled by source syntax, but it remains an ordinary
    // exported function subjected to Types, Analyze and independent Verify.
    for (module, _) in &specialized {
        for item in &module.items {
            let s::ItemKind::Aggregate(a) = &item.kind else {
                continue;
            };
            if !item.public || a.kind != s::AggregateKind::Struct {
                continue;
            }
            for field in &a.fields {
                if !field.public || field.manages || field.default.is_none() {
                    continue;
                }
                let cost = a
                    .name
                    .len()
                    .saturating_add(field.name.len())
                    .saturating_mul(2)
                    .saturating_add(64);
                remaining.max_analysis_steps = remaining
                    .max_analysis_steps
                    .checked_sub(cost)
                    .ok_or_else(|| {
                        vec![Diagnostic::limit(
                            Phase::Expand,
                            "project default helper budget exceeded",
                            Some(field.span),
                        )]
                    })?;
                let helper = records::project_default_name(&a.name, &field.name);
                defaults.insert((a.name.clone(), field.name.clone()), (module.id, helper));
            }
        }
    }
    for ((source, (module, module_imports)), source_imports) in
        sources.iter().zip(&specialized).zip(imports)
    {
        let mut expanded = expand_inner_project(
            source,
            module,
            true,
            sources,
            (
                &declarations,
                &project_modules,
                source_imports,
                &products.trait_signatures,
                products.metered(),
                &products.member_patterns,
                &products.dynamic,
                &products.storage,
            ),
            (&remaining, &remaining),
            Some(&defaults),
        )?;
        // New member constraints share the project's actual remaining allowance.
        remaining.max_analysis_steps = remaining
            .max_analysis_steps
            .checked_sub(expanded.member_owner_work)
            .ok_or_else(|| {
                vec![Diagnostic::limit(
                    Phase::Expand,
                    "project member owner work budget exceeded",
                    Some(module.span),
                )]
            })?;
        remaining.max_nodes = remaining
            .max_nodes
            .checked_sub(expanded.member_owner_nodes)
            .ok_or_else(|| {
                vec![Diagnostic::limit(
                    Phase::Expand,
                    "project member owner node budget exceeded",
                    Some(module.span),
                )]
            })?;
        // New project trait work shares the project's remaining allowance; a
        // later implementation module does not receive those steps again.
        remaining.max_analysis_steps = remaining
            .max_analysis_steps
            .checked_sub(expanded.project_trait_work)
            .ok_or_else(|| {
                vec![Diagnostic::limit(
                    Phase::Expand,
                    "project trait work budget exceeded",
                    Some(module.span),
                )]
            })?;
        remaining.max_nodes = remaining
            .max_nodes
            .checked_sub(expanded.project_trait_nodes)
            .ok_or_else(|| {
                vec![Diagnostic::limit(
                    Phase::Expand,
                    "project trait instance node budget exceeded",
                    Some(module.span),
                )]
            })?;
        remaining.max_nodes = remaining
            .max_nodes
            .checked_sub(expanded.source_map.entries.len())
            .ok_or_else(|| {
                vec![Diagnostic::limit(
                    Phase::Expand,
                    "project source-map node budget exceeded",
                    Some(module.span),
                )]
            })?;
        let map_work = expanded
            .source_map
            .entries
            .iter()
            .try_fold(sources.len(), |work, entry| {
                work.checked_add(entry.related.len().saturating_add(1))
            });
        remaining.max_analysis_steps = map_work
            .and_then(|work| remaining.max_analysis_steps.checked_sub(work))
            .ok_or_else(|| {
                vec![Diagnostic::limit(
                    Phase::Expand,
                    "project source-map work budget exceeded",
                    Some(module.span),
                )]
            })?;
        if !products.dynamic.is_empty()
            || !expanded.closure_context_emissions.is_empty()
            || !expanded.callable_context_emissions.is_empty()
        {
            products.allowance = Some(remaining);
            products.bind_dynamic_targets(module, &expanded, limits)?;
            products.bind_closure_contexts(module, &mut expanded, limits)?;
            products.bind_callable_contexts(module, &mut expanded, limits)?;
            remaining = products
                .allowance
                .clone()
                .expect("dynamic target binding preserves allowance");
        }
        let mut exports = source_exports(
            module,
            &expanded.core,
            &expanded.imported_public_traits,
            &expanded.trait_method_names,
        );
        exports.extend(expanded.generated_exports);
        let mut resolved_imports = module_imports.clone();
        resolved_imports.extend(expanded.generated_imports);
        result.push((
            ProgramModule {
                core: expanded.core,
                imports: resolved_imports,
                exports,
            },
            expanded.source_map,
        ));
    }
    let mut product_cores = result
        .iter_mut()
        .map(|(module, map)| (&mut module.core, map))
        .collect::<Vec<_>>();
    if products.metered() {
        products.allowance = Some(remaining);
    }
    products.finish_closure_contexts(limits)?;
    products.finish_callable_contexts(limits)?;
    products.publish(&mut product_cores, sources, limits)?;
    Ok(result)
}

fn debit_trait_lowering(
    expanded: &ExpandedModule,
    remaining: &mut Limits,
    source_count: usize,
) -> CompileResult<()> {
    let map_work = expanded
        .source_map
        .entries
        .iter()
        .try_fold(source_count, |work, entry| {
            work.checked_add(entry.related.len().saturating_add(1))
        });
    let work = map_work
        .and_then(|work| work.checked_add(expanded.member_owner_work))
        .and_then(|work| work.checked_add(expanded.project_trait_work));
    remaining.max_analysis_steps = work
        .and_then(|work| remaining.max_analysis_steps.checked_sub(work))
        .ok_or_else(|| {
            vec![Diagnostic::limit(
                Phase::Expand,
                "trait expansion request work budget exceeded",
                None,
            )]
        })?;
    let nodes = expanded
        .member_owner_nodes
        .checked_add(expanded.project_trait_nodes)
        .and_then(|nodes| nodes.checked_add(expanded.source_map.entries.len()));
    remaining.max_nodes = nodes
        .and_then(|nodes| remaining.max_nodes.checked_sub(nodes))
        .ok_or_else(|| {
            vec![Diagnostic::limit(
                Phase::Expand,
                "trait expansion request node budget exceeded",
                None,
            )]
        })?;
    Ok(())
}

#[allow(clippy::too_many_arguments)] // Private immutable lowering contexts remain explicit.
fn expand_inner(
    source: &SourceUnit,
    module: &s::Module,
    imports_resolved: bool,
    owner_limits: &Limits,
    signatures: &generics::trait_instances::Signatures,
    metered_trait_instances: bool,
    member_patterns: &BTreeMap<String, s::TypeRef>,
    dynamic_catalog: &generics::dynamic::Catalog,
    storage_catalog: &stored_types::Catalog,
) -> CompileResult<ExpandedModule> {
    let declarations = declaration_index(std::iter::once(module));
    let legacy_limits = Limits::default();
    let lowering_limits = if metered_trait_instances {
        owner_limits
    } else {
        &legacy_limits
    };
    expand_inner_project(
        source,
        module,
        imports_resolved,
        std::slice::from_ref(source),
        (
            &declarations,
            &[module],
            &[],
            signatures,
            metered_trait_instances,
            member_patterns,
            dynamic_catalog,
            storage_catalog,
        ),
        (lowering_limits, owner_limits),
        None,
    )
}

// Collect once for the whole immutable syntax snapshot. Re-scanning every
// project's declaration list once per module is quadratic in module count.
// First declaration wins exactly as in the original ordered traversal.
type Declarations<'a> = BTreeMap<String, &'a s::Aggregate>;
type ProjectLowering<'a> = (
    &'a Declarations<'a>,
    &'a [&'a s::Module],
    &'a [CoreImport],
    &'a generics::trait_instances::Signatures,
    bool,
    &'a BTreeMap<String, s::TypeRef>,
    &'a generics::dynamic::Catalog,
    &'a stored_types::Catalog,
);
fn declaration_index<'a>(modules: impl IntoIterator<Item = &'a s::Module>) -> Declarations<'a> {
    let mut declarations = BTreeMap::new();
    for item in modules.into_iter().flat_map(|module| &module.items) {
        if let s::ItemKind::Aggregate(a) = &item.kind {
            declarations.entry(a.name.clone()).or_insert(a);
        }
    }
    declarations
}

fn expand_inner_project(
    source: &SourceUnit,
    module: &s::Module,
    imports_resolved: bool,
    sources: &[SourceUnit],
    project: ProjectLowering<'_>,
    budgets: (&Limits, &Limits),
    project_defaults: Option<&ProjectDefaults>,
) -> CompileResult<ExpandedModule> {
    let (limits, owner_limits) = budgets;
    let (
        declarations,
        project_modules,
        imports,
        signatures,
        metered_trait_instances,
        member_patterns,
        dynamic_catalog,
        storage_catalog,
    ) = project;
    let mut e = Expander::new(
        module.id,
        imports_resolved,
        limits,
        project_defaults,
        declarations,
    );
    // Preserve the legacy single-module lowering allowance while binding the
    // newly introduced owner work to the caller's actual remaining budget.
    e.member_owner_limit = owner_limits.max_analysis_steps;
    e.member_owner_nodes = owner_limits.max_nodes;
    e.member_owner_depth = owner_limits.max_call_depth;
    for item in &module.items {
        if let s::ItemKind::Trait(t) = &item.kind {
            e.traits.entry(t.name.clone()).or_insert(t);
        }
    }
    e.enum_sources = project_modules;
    e.member_patterns = Some(member_patterns);
    e.dynamic_catalog = Some(dynamic_catalog);
    e.storage_catalog = Some(storage_catalog);
    e.collect_resource_drops(module);
    e.collect_static_traits(
        module,
        project_modules,
        imports,
        limits,
        signatures,
        metered_trait_instances,
    );
    // A public default is a declaration: check its body even when a particular
    // consumer explicitly supplies that field. Private/local defaults retain
    // their existing lazy lowering.
    if let Some(defaults) = project_defaults {
        for item in &module.items {
            if let s::ItemKind::Aggregate(a) = &item.kind {
                for field in &a.fields {
                    if defaults.contains_key(&(a.name.clone(), field.name.clone())) {
                        e.default_helper(&a.name, field);
                    }
                }
            }
        }
    }
    for (ordinal, item) in module.items.iter().enumerate() {
        e.source_item_ordinal = ordinal;
        e.item(item, None, None, None);
    }
    if !e.diagnostics.is_empty() {
        return Err(e.diagnostics);
    }
    e.source_map.validate(sources, limits)?;
    let core = CoreModule {
        life_declarations: Vec::new(),
        life_instances: Vec::new(),
        closure_life_contexts: Vec::new(),
        dyn_declarations: Vec::new(),
        dyn_interfaces: Vec::new(),
        dyn_vtables: Vec::new(),
        aggregate_templates: Vec::new(),
        aggregate_applications: Vec::new(),
        schema_version: SCHEMA_VERSION,
        module_id: module.id,
        source_fingerprint: source.fingerprint(),
        functions: e.functions,
        aggregates: e.aggregates,
        enums: e.enums,
        unsupported: e.unsupported,
    };
    Ok(ExpandedModule {
        core,
        source_map: e.source_map,
        generated_imports: e.generated_imports.into_values().collect(),
        generated_exports: e.generated_exports,
        imported_public_traits: e.imported_public_traits,
        trait_method_names: e.trait_method_names,
        dyn_method_emissions: e.dyn_method_emissions,
        closure_context_emissions: e.closure_context_emissions,
        callable_context_emissions: e.callable_context_emissions,
        project_trait_work: e.project_trait_work,
        project_trait_nodes: e.project_trait_nodes,
        member_owner_work: e.member_owner_work,
        member_owner_nodes: owner_limits.max_nodes - e.member_owner_nodes,
    })
}
struct Expander<'a> {
    callable_work: usize,
    callable_initial_work: usize,
    member_owner_limit: usize,
    member_owner_work: usize,
    member_owner_nodes: usize,
    member_owner_depth: usize,
    module_id: zil_base::ModuleId,
    project_defaults: Option<&'a ProjectDefaults>,
    generated_imports: BTreeMap<zil_base::ModuleId, CoreImport>,
    generated_exports: Vec<NodeId>,
    imports_resolved: bool,
    next: u32,
    next_temporary: u32,
    unsupported: Vec<UnsupportedFeature>,
    diagnostics: Vec<Diagnostic>,
    diagnosed_attributes: BTreeSet<Span>,
    functions: Vec<CoreFunction>,
    aggregates: Vec<CoreAggregate>,
    enums: Vec<CoreEnum>,
    enum_sources: &'a [&'a s::Module],
    member_patterns: Option<&'a BTreeMap<String, s::TypeRef>>,
    dynamic_catalog: Option<&'a generics::dynamic::Catalog>,
    storage_catalog: Option<&'a stored_types::Catalog>,
    dyn_method_emissions: Vec<(usize, usize, NodeId)>,
    closure_context_emissions: Vec<ClosureContextEmission>,
    callable_context_emissions: Vec<CallableContextEmission>,
    named_match: bool,
    declarations: &'a Declarations<'a>,
    default_helpers: BTreeMap<(String, String), String>,
    traits: BTreeMap<String, &'a s::TraitDecl>,
    imported_public_traits: BTreeSet<String>,
    trait_method_names: BTreeMap<(usize, usize), String>,
    project_trait_work: usize,
    project_trait_nodes: usize,
    static_traits: BTreeSet<String>,
    static_trait_impls: BTreeSet<usize>,
    source_item_ordinal: usize,
    current_function: Option<NodeId>,
    source_map: SourceMap,
    node_maps: BTreeMap<NodeId, SourceMapId>,
    markers: BTreeMap<Span, SourceMapId>,
    parent_map: Option<SourceMapId>,
    lexical_receiver: Option<Span>,
    self_type: Option<Type>,
    return_type: Option<Type>,
    drop_implementations: BTreeMap<String, String>,
}
impl<'a> Expander<'a> {
    fn new(
        module_id: zil_base::ModuleId,
        imports_resolved: bool,
        limits: &Limits,
        project_defaults: Option<&'a ProjectDefaults>,
        declarations: &'a Declarations<'a>,
    ) -> Self {
        Self {
            imports_resolved,
            module_id,
            callable_work: limits.max_analysis_steps,
            callable_initial_work: limits.max_analysis_steps,
            member_owner_limit: limits.max_analysis_steps,
            member_owner_work: 0,
            member_owner_nodes: limits.max_nodes,
            member_owner_depth: limits.max_call_depth,
            project_defaults,
            generated_imports: BTreeMap::new(),
            generated_exports: Vec::new(),
            next: 0,
            next_temporary: 0,
            unsupported: Vec::new(),
            diagnostics: Vec::new(),
            diagnosed_attributes: BTreeSet::new(),
            functions: Vec::new(),
            aggregates: Vec::new(),
            enums: Vec::new(),
            enum_sources: &[],
            member_patterns: None,
            dynamic_catalog: None,
            storage_catalog: None,
            dyn_method_emissions: Vec::new(),
            closure_context_emissions: Vec::new(),
            callable_context_emissions: Vec::new(),
            named_match: false,
            declarations,
            default_helpers: BTreeMap::new(),
            traits: BTreeMap::new(),
            imported_public_traits: BTreeSet::new(),
            trait_method_names: BTreeMap::new(),
            project_trait_work: 0,
            project_trait_nodes: 0,
            static_traits: BTreeSet::new(),
            static_trait_impls: BTreeSet::new(),
            source_item_ordinal: 0,
            current_function: None,
            source_map: SourceMap::new(module_id),
            node_maps: BTreeMap::new(),
            markers: BTreeMap::new(),
            parent_map: None,
            lexical_receiver: None,
            self_type: None,
            return_type: None,
            drop_implementations: BTreeMap::new(),
        }
    }
}
impl Expander<'_> {
    fn id(&mut self) -> NodeId {
        let id = NodeId(self.next);
        self.next += 1;
        id
    }
    fn unsupported(&mut self, text: impl Into<String>, span: Span) {
        self.unsupported.push(UnsupportedFeature {
            feature: text.into(),
            span: Some(span),
        })
    }
    fn unexecuted_attributes(&mut self, attributes: &[s::Attribute], attachment: &str) {
        // Attribute execution/configuration has not materialized this AST. Core
        // has no place to preserve these requests, so dropping them would change
        // the signature or layout (even for a cfg attachment known to be false).
        for attribute in attributes {
            if !self.diagnosed_attributes.insert(attribute.span) {
                continue;
            }
            self.diagnostics.push(Diagnostic::unsupported(
                Phase::Expand,
                format!(
                    "unexecuted {attachment} attribute `{}`",
                    attribute.path.join(".")
                ),
                Some(attribute.span),
            ));
        }
    }
    fn expr_node(&mut self, kind: CoreExprKind, span: Span) -> CoreExpr {
        let id = self.id();
        self.map_node(id, span, ExpansionRole::Source, Vec::new());
        CoreExpr { id, kind, span }
    }
    fn statement(&mut self, kind: CoreStatementKind, span: Span) -> CoreStatement {
        let id = self.id();
        self.map_node(id, span, ExpansionRole::Source, Vec::new());
        CoreStatement { id, kind, span }
    }
    fn item(
        &mut self,
        item: &s::Item,
        prefix: Option<&str>,
        host: Option<&Type>,
        default: Option<&s::Param>,
    ) {
        self.unexecuted_attributes(&item.attributes, "declaration");
        match &item.kind {
            s::ItemKind::Function(f) => {
                self.function(f, prefix, host, default);
            }
            s::ItemKind::Aggregate(a) => {
                for field in &a.fields {
                    self.unexecuted_attributes(&field.attributes, "field");
                }
                if !a.generics.is_empty() {
                    self.unsupported("generic aggregate", item.span)
                }
                let id = self.id();
                self.map_node(id, item.span, ExpansionRole::Source, Vec::new());
                let fields = a
                    .fields
                    .iter()
                    .map(|f| CoreField {
                        name: f.name.clone(),
                        ty: self.dyn_type_ref(&f.ty),
                        public: f.public,
                        manages: f.manages,
                        span: f.span,
                    })
                    .collect();
                self.aggregates.push(CoreAggregate {
                    teardown: self.drop_implementations.get(&a.name).cloned(),
                    id,
                    name: a.name.clone(),
                    kind: match a.kind {
                        s::AggregateKind::Struct => AggregateKind::Struct,
                        s::AggregateKind::Resource => AggregateKind::Resource,
                        s::AggregateKind::Package => AggregateKind::Package,
                    },
                    fields,
                    span: item.span,
                });
                let host = Type::Named(a.name.clone());
                for m in &a.methods {
                    self.item(m, Some(&a.name), Some(&host), None)
                }
            }
            s::ItemKind::Impl(i) => {
                if self.resource_drop(i) {
                    return;
                }
                if i.trait_type.is_some()
                    && !self.static_trait_impls.contains(&self.source_item_ordinal)
                {
                    self.unsupported("trait implementation resolution", item.span)
                }
                let name = type_path(&i.target).unwrap_or_else(|| "$unsupported".into());
                let host = type_ref(&i.target);
                for (member_ordinal, m) in i.members.iter().enumerate() {
                    let inherited = if let Some(trait_type) = &i.trait_type {
                        self.trait_receiver(trait_type, m)
                    } else {
                        i.receiver_default.clone()
                    };
                    let first = self.functions.len();
                    self.item(m, Some(&name), Some(&host), inherited.as_ref());
                    if i.trait_type.is_some()
                        && matches!(m.kind, s::ItemKind::Function(_))
                        && self.functions.len() > first
                    {
                        let target = self.functions.last().expect("emitted method").id;
                        self.record_dyn_method(member_ordinal, target, m.span);
                    }
                    if matches!(m.kind, s::ItemKind::Function(_))
                        && let Some(target) = self
                            .trait_method_names
                            .get(&(self.source_item_ordinal, member_ordinal))
                    {
                        // function() appends its own function after any
                        // helpers emitted while lowering the body. The result
                        // of this invocation is independent of source spans.
                        if self.functions.len() > first {
                            self.functions.last_mut().expect("emitted method").name =
                                format!("{name}.{target}");
                        }
                    }
                }
            }
            s::ItemKind::Import(import) if self.imports_resolved => {
                if let Some(name) = import
                    .alias
                    .as_deref()
                    .or_else(|| import.path.last().map(String::as_str))
                {
                    self.scoped_binding(name, item.span);
                }
            }
            s::ItemKind::Import(_) => {
                self.unsupported("module import loading requires a project input", item.span)
            }
            s::ItemKind::Const(_) => self.unsupported("module constant evaluation", item.span),
            s::ItemKind::TypeAlias(_) => self.unsupported("type aliases", item.span),
            s::ItemKind::Enum(declaration) => self.enum_declaration(declaration, item.span),
            s::ItemKind::Trait(t) => {
                if !self.static_traits.contains(&t.name) {
                    self.unsupported(
                        "trait declarations outside the finite local subset",
                        item.span,
                    );
                }
            }
            s::ItemKind::Extern { abi, function } => {
                self.foreign(abi, function, item.public, prefix, host)
            }
            s::ItemKind::Meta(_) => self.unsupported("compile-time item generation", item.span),
        }
    }
    fn param(&mut self, p: &s::Param) -> CoreParameter {
        self.scoped_binding(&p.name, p.span);
        self.unexecuted_attributes(&p.attributes, "parameter");
        CoreParameter {
            name: p.name.clone(),
            ty: self.dyn_type_ref(&p.ty),
            mode: mode(p),
            span: p.span,
        }
    }
    fn scoped_binding(&mut self, name: &str, span: Span) {
        if registration_name(name)
            || task_name(name)
            || sync_name(name)
            || name == "Option"
            || name == "c_with_callback"
            || name == "thread_scope"
        {
            self.diagnostics.push(primitive_name_error(name, span));
        }
    }
    fn block(&mut self, b: &s::Block) -> CoreBlock {
        CoreBlock {
            statements: b.statements.iter().map(|s| self.stmt(s)).collect(),
            span: b.span,
        }
    }
    /// Stabilize every index before reading the old value or evaluating the RHS.
    /// The duplicated place consists only of local names and prepared indices;
    /// no source expression is executed twice and every generated node is fresh.
    fn compound_place(
        &mut self,
        place: CoreExpr,
        bindings: &mut Vec<CoreStatement>,
    ) -> Option<(CoreExpr, CoreExpr)> {
        let span = place.span;
        let (target, read) = match place.kind {
            CoreExprKind::Name(name) => {
                (CoreExprKind::Name(name.clone()), CoreExprKind::Name(name))
            }
            CoreExprKind::Unary {
                op: UnaryOp::Dereference,
                operand,
            } => {
                // A stable address name is an access view. Introducing a new
                // exclusive child here would incorrectly freeze `*p += *p`.
                // An effectful address expression instead produces one owned
                // temporary, evaluated before the old value and the RHS.
                let operand_span = operand.span;
                let name = if let CoreExprKind::Name(name) = &operand.kind {
                    name.clone()
                } else {
                    let name = self.temporary("place_address");
                    bindings.push(self.statement(
                        CoreStatementKind::Binding {
                            name: name.clone(),
                            mutable: false,
                            ty: None,
                            initializer: Some(*operand),
                        },
                        operand_span,
                    ));
                    name
                };
                (
                    CoreExprKind::Unary {
                        op: UnaryOp::Dereference,
                        operand: Box::new(self.temporary_read(&name, operand_span)),
                    },
                    CoreExprKind::Unary {
                        op: UnaryOp::Dereference,
                        operand: Box::new(self.temporary_read(&name, operand_span)),
                    },
                )
            }
            CoreExprKind::Member {
                object,
                member,
                owner,
            } => {
                let (object, read_object) = self.compound_place(*object, bindings)?;
                (
                    CoreExprKind::Member {
                        object: Box::new(object),
                        member: member.clone(),
                        owner: owner.clone(),
                    },
                    CoreExprKind::Member {
                        object: Box::new(read_object),
                        member,
                        owner,
                    },
                )
            }
            CoreExprKind::Index { object, index } => {
                let (object, read_object) = self.compound_place(*object, bindings)?;
                let index_span = index.span;
                let name = self.temporary("place_index");
                bindings.push(self.statement(
                    CoreStatementKind::Binding {
                        name: name.clone(),
                        mutable: false,
                        ty: None,
                        initializer: Some(*index),
                    },
                    index_span,
                ));
                let index = self.temporary_read(&name, index_span);
                let read_index = self.temporary_read(&name, index_span);
                (
                    CoreExprKind::Index {
                        object: Box::new(object),
                        index: Box::new(index),
                    },
                    CoreExprKind::Index {
                        object: Box::new(read_object),
                        index: Box::new(read_index),
                    },
                )
            }
            _ => return None,
        };
        let target = self.expr_node(target, span);
        let read = self.expr_node(read, span);
        self.role(read.id, ExpansionRole::TemporaryRead, Vec::new());
        Some((target, read))
    }
    fn tuple_binding_pattern(pattern: &s::Pattern) -> bool {
        match &pattern.kind {
            s::PatternKind::Name(_) | s::PatternKind::Wildcard => true,
            s::PatternKind::Tuple(fields) => fields.iter().all(Self::tuple_binding_pattern),
            s::PatternKind::Reference { pattern, .. }
            | s::PatternKind::ErrorArm { pattern, .. } => Self::tuple_binding_pattern(pattern),
            _ => false,
        }
    }
    fn stmt(&mut self, s: &s::Stmt) -> CoreStatement {
        let kind = match &s.kind {
            s::StmtKind::Binding {
                mutable,
                pattern,
                ty,
                value,
            } => {
                if matches!(
                    pattern.kind,
                    s::PatternKind::Tuple(_)
                        | s::PatternKind::Reference { .. }
                        | s::PatternKind::ErrorArm { .. }
                ) {
                    if !Self::tuple_binding_pattern(pattern) {
                        return self.statement(
                            CoreStatementKind::Unsupported(
                                "tuple binding requires names, discards and tuple patterns".into(),
                            ),
                            s.span,
                        );
                    }
                    let ty = ty.as_ref().map(|t| self.dyn_type_ref(t));
                    let initializer = self.expr_expected(value, ty.as_ref());
                    let pattern = self.pattern(pattern);
                    return self.statement(
                        CoreStatementKind::TupleBinding {
                            pattern,
                            mutable: *mutable,
                            ty,
                            initializer,
                        },
                        s.span,
                    );
                }
                let name = match &pattern.kind {
                    s::PatternKind::Name(n) => n.clone(),
                    s::PatternKind::Wildcard => format!("$discard{}", self.next),
                    _ => {
                        return self.statement(
                            CoreStatementKind::Unsupported("binding destructuring".into()),
                            s.span,
                        );
                    }
                };
                self.scoped_binding(&name, pattern.span);
                let ty = ty.as_ref().map(|t| self.dyn_type_ref(t));
                let initializer = Some(self.expr_expected(value, ty.as_ref()));
                CoreStatementKind::Binding {
                    name,
                    mutable: *mutable,
                    ty,
                    initializer,
                }
            }
            s::StmtKind::Return(v) => {
                let expected = self.return_type.clone();
                CoreStatementKind::Return(
                    v.as_ref().map(|v| self.expr_expected(v, expected.as_ref())),
                )
            }
            s::StmtKind::Expression(v) => CoreStatementKind::Expression(self.expr(v)),
            s::StmtKind::Assign { target, op, value } => {
                let target = self.expr(target);
                let value = self.expr(value);
                if *op == s::AssignOp::Set {
                    CoreStatementKind::Assign { target, value }
                } else {
                    let mut statements = Vec::new();
                    let Some((target, read)) = self.compound_place(target, &mut statements) else {
                        return self.statement(
                            CoreStatementKind::Unsupported(
                                "compound assignment requires a local projection".into(),
                            ),
                            s.span,
                        );
                    };
                    let binary = match op {
                        s::AssignOp::Add => BinaryOp::Add,
                        s::AssignOp::Subtract => BinaryOp::Subtract,
                        s::AssignOp::Multiply => BinaryOp::Multiply,
                        s::AssignOp::Divide => BinaryOp::Divide,
                        s::AssignOp::Remainder => BinaryOp::Remainder,
                        s::AssignOp::BitAnd => BinaryOp::BitAnd,
                        s::AssignOp::BitOr => BinaryOp::BitOr,
                        s::AssignOp::BitXor => BinaryOp::BitXor,
                        s::AssignOp::Set => unreachable!(),
                    };
                    let value = self.expr_node(
                        CoreExprKind::Binary {
                            op: binary,
                            left: Box::new(read),
                            right: Box::new(value),
                        },
                        s.span,
                    );
                    let assignment = CoreStatementKind::Assign { target, value };
                    if statements.is_empty() {
                        assignment
                    } else {
                        statements.push(self.statement(assignment, s.span));
                        CoreStatementKind::Block(CoreBlock {
                            statements,
                            span: s.span,
                        })
                    }
                }
            }
            s::StmtKind::If {
                condition,
                then_body,
                else_body,
            } => {
                // Expand the tested value before either body, preserving source order.
                let value = match condition {
                    s::Condition::Expression(v) | s::Condition::Let { value: v, .. } => {
                        self.expr(v)
                    }
                };
                let then_body = self.block(then_body);
                let else_body = else_body.as_ref().map(|b| match b {
                    s::ElseBranch::Block(b) => self.block(b),
                    s::ElseBranch::If(i) => CoreBlock {
                        statements: vec![self.stmt(i)],
                        span: i.span,
                    },
                });
                match condition {
                    s::Condition::Expression(_) => CoreStatementKind::If {
                        condition: value,
                        then_body,
                        else_body,
                    },
                    s::Condition::Let { pattern, .. } => CoreStatementKind::IfLet {
                        pattern: self.pattern(pattern),
                        value,
                        then_body,
                        else_body,
                    },
                }
            }
            s::StmtKind::While { condition, body } => match condition {
                s::Condition::Expression(v) => CoreStatementKind::While {
                    condition: self.expr(v),
                    body: self.block(body),
                },
                s::Condition::Let { pattern, value } => CoreStatementKind::WhileLet {
                    pattern: self.pattern(pattern),
                    value: self.expr(value),
                    body: self.block(body),
                },
            },
            s::StmtKind::Loop(body) => {
                let condition = self.expr_node(CoreExprKind::Bool(true), s.span);
                CoreStatementKind::While {
                    condition,
                    body: self.block(body),
                }
            }
            s::StmtKind::For {
                pattern,
                iterable,
                body,
            } => {
                let s::PatternKind::Name(name) = &pattern.kind else {
                    return self.statement(
                        CoreStatementKind::Unsupported("for pattern other than a name".into()),
                        s.span,
                    );
                };
                let mut iterable = iterable;
                while let s::ExprKind::Group(inner) = &iterable.kind {
                    iterable = inner;
                }
                let s::ExprKind::Binary {
                    op: s::BinaryOp::Range,
                    left,
                    right,
                } = &iterable.kind
                else {
                    return self.statement(
                        CoreStatementKind::Unsupported(
                            "for iterable other than an integer range".into(),
                        ),
                        s.span,
                    );
                };
                self.scoped_binding(name, pattern.span);
                CoreStatementKind::ForRange {
                    name: name.clone(),
                    start: self.expr(left),
                    end: self.expr(right),
                    body: self.block(body),
                }
            }
            s::StmtKind::Defer(body) => CoreStatementKind::Defer(self.block(body)),
            s::StmtKind::Unsafe(body) => CoreStatementKind::Unsafe(self.block(body)),
            s::StmtKind::Break => CoreStatementKind::Break,
            s::StmtKind::Continue => CoreStatementKind::Continue,
            s::StmtKind::Match { value, arms } => self.match_statement(value, arms),
            _ => CoreStatementKind::Unsupported(
                "statement requires a later language capability".into(),
            ),
        };
        self.statement(kind, s.span)
    }
    fn generated_ascription(&mut self, e: &s::Expr) -> CoreExprKind {
        let s::ExprKind::Ascribed { ty, value } = &e.kind else {
            unreachable!("ascription dispatch")
        };
        let ty = self.dyn_type_ref(ty);
        let value = self.expr_expected(value, Some(&ty));
        CoreExprKind::Ascribed {
            ty,
            value: Box::new(value),
        }
    }
    fn expr(&mut self, e: &s::Expr) -> CoreExpr {
        self.expr_expected(e, None)
    }
    fn expr_expected(&mut self, e: &s::Expr, expected: Option<&Type>) -> CoreExpr {
        // A thin variant dispatcher keeps unrelated reconstruction/diagnostic
        // storage off every recursive lowering edge. Group/receiver/record/
        // pipeline retain their original early-return node behavior.
        match &e.kind {
            s::ExprKind::ReceiverMember(n) => self.receiver_member(n, e.span),
            s::ExprKind::Group(v) => self.expr_expected(v, expected),
            s::ExprKind::Pipeline { input, target } => self.pipeline(e.span, input, target),
            s::ExprKind::Record { ty, base, fields } => {
                if let Some(value) =
                    self.record_enum(e.span, ty.as_deref(), base.as_deref(), fields)
                {
                    return value;
                }
                self.record(e.span, ty.as_deref(), base.as_deref(), fields, expected)
            }
            s::ExprKind::Ascribed { .. } => self.lower_expr_ascribed(e, expected),
            s::ExprKind::Array(_) => self.lower_expr_array(e, expected),
            s::ExprKind::Index { .. } => self.lower_expr_index(e, expected),
            s::ExprKind::Slice { .. } => self.lower_expr_slice(e, expected),
            s::ExprKind::Tuple(_) => self.lower_expr_tuple(e, expected),
            s::ExprKind::Unary { .. } => self.lower_expr_unary(e, expected),
            s::ExprKind::Binary { .. } => self.lower_expr_binary(e, expected),
            s::ExprKind::Call { .. } => self.lower_expr_call(e, expected),
            s::ExprKind::Member { .. } => self.lower_expr_member(e, expected),
            s::ExprKind::Closure { .. } => self.lower_expr_closure(e, expected),
            s::ExprKind::If { .. } => self.lower_expr_if(e, expected),
            s::ExprKind::Try(_) => self.lower_expr_try(e, expected),
            s::ExprKind::MetaEval(_) => self.lower_expr_metaeval(e, expected),
            _ => self.lower_expr_leaf(e),
        }
    }

    #[inline(never)]
    fn lower_expr_leaf(&mut self, e: &s::Expr) -> CoreExpr {
        let kind = match &e.kind {
            s::ExprKind::Literal(l) => match l {
                s::Literal::Integer(v) => CoreExprKind::Integer(v.clone()),
                s::Literal::Float(v) if v.starts_with("@bits:") => {
                    let parsed = self.enum_meter(|meter| {
                        meter.text(v, e.span)?;
                        zil_base::numeric::FloatValue::from_generated_spelling(v).map_err(|_| Diagnostic::error(Phase::Expand, "E-FLOAT-LITERAL", Some(e.span), "generated floating constant requires fixed-width canonical hexadecimal bits"))
                    });
                    match parsed {
                        Ok(zil_base::numeric::FloatValue::F32(bits)) => {
                            CoreExprKind::Literal(Constant::F32(bits))
                        }
                        Ok(zil_base::numeric::FloatValue::F64(bits)) => {
                            CoreExprKind::Literal(Constant::F64(bits))
                        }
                        Err(error) => {
                            self.diagnostics.push(error);
                            CoreExprKind::Unit
                        }
                    }
                }
                s::Literal::Float(v) => CoreExprKind::Float(v.clone()),
                s::Literal::Bool(v) => CoreExprKind::Bool(*v),
                s::Literal::String(v) => CoreExprKind::String(v.clone()),
                _ => CoreExprKind::Unsupported("floating/character literal typing".into()),
            },
            s::ExprKind::Name(n) => CoreExprKind::Name(n.clone()),
            _ => {
                CoreExprKind::Unsupported("expression requires a later language capability".into())
            }
        };
        self.expr_node(kind, e.span)
    }

    #[inline(never)]
    fn lower_expr_ascribed(&mut self, e: &s::Expr, _expected: Option<&Type>) -> CoreExpr {
        let kind = match &e.kind {
            s::ExprKind::Ascribed { .. } => self.generated_ascription(e),
            _ => unreachable!("lowering expression dispatcher"),
        };
        self.expr_node(kind, e.span)
    }

    #[inline(never)]
    fn lower_expr_array(&mut self, e: &s::Expr, _expected: Option<&Type>) -> CoreExpr {
        let kind = match &e.kind {
            s::ExprKind::Array(values) => {
                CoreExprKind::Array(values.iter().map(|v| self.expr(v)).collect())
            }
            _ => unreachable!("lowering expression dispatcher"),
        };
        self.expr_node(kind, e.span)
    }

    #[inline(never)]
    fn lower_expr_index(&mut self, e: &s::Expr, _expected: Option<&Type>) -> CoreExpr {
        let kind = match &e.kind {
            s::ExprKind::Index { object, index } => CoreExprKind::Index {
                object: Box::new(self.expr(object)),
                index: Box::new(self.expr(index)),
            },
            _ => unreachable!("lowering expression dispatcher"),
        };
        self.expr_node(kind, e.span)
    }

    #[inline(never)]
    fn lower_expr_slice(&mut self, e: &s::Expr, _expected: Option<&Type>) -> CoreExpr {
        let kind = match &e.kind {
            s::ExprKind::Slice { object, start, end } => CoreExprKind::Slice {
                object: Box::new(self.expr(object)),
                start: start.as_ref().map(|value| Box::new(self.expr(value))),
                end: end.as_ref().map(|value| Box::new(self.expr(value))),
            },
            _ => unreachable!("lowering expression dispatcher"),
        };
        self.expr_node(kind, e.span)
    }

    #[inline(never)]
    fn lower_expr_tuple(&mut self, e: &s::Expr, _expected: Option<&Type>) -> CoreExpr {
        let kind = match &e.kind {
            s::ExprKind::Tuple(values) if values.is_empty() => CoreExprKind::Unit,
            s::ExprKind::Tuple(values) => {
                CoreExprKind::Tuple(values.iter().map(|v| self.expr(v)).collect())
            }
            _ => unreachable!("lowering expression dispatcher"),
        };
        self.expr_node(kind, e.span)
    }

    #[inline(never)]
    fn lower_expr_unary(&mut self, e: &s::Expr, _expected: Option<&Type>) -> CoreExpr {
        let kind = match &e.kind {
            s::ExprKind::Unary { op, operand } => CoreExprKind::Unary {
                op: match op {
                    s::UnaryOp::Not => UnaryOp::Not,
                    s::UnaryOp::Negate => UnaryOp::Negate,
                    s::UnaryOp::BitNot => UnaryOp::BitNot,
                    s::UnaryOp::Dereference => UnaryOp::Dereference,
                    s::UnaryOp::Borrow => UnaryOp::Borrow,
                    s::UnaryOp::BorrowMut => UnaryOp::BorrowMut,
                    s::UnaryOp::Give => UnaryOp::Give,
                },
                operand: Box::new(self.expr(operand)),
            },
            _ => unreachable!("lowering expression dispatcher"),
        };
        self.expr_node(kind, e.span)
    }

    #[inline(never)]
    fn lower_expr_binary(&mut self, e: &s::Expr, _expected: Option<&Type>) -> CoreExpr {
        let kind = match &e.kind {
            s::ExprKind::Binary { op, left, right } => {
                let Some(op) = binary(*op) else {
                    return self.expr_node(CoreExprKind::Unsupported("range value".into()), e.span);
                };
                CoreExprKind::Binary {
                    op,
                    left: Box::new(self.expr(left)),
                    right: Box::new(self.expr(right)),
                }
            }
            _ => unreachable!("lowering expression dispatcher"),
        };
        self.expr_node(kind, e.span)
    }

    #[inline(never)]
    fn lower_expr_call(&mut self, e: &s::Expr, _expected: Option<&Type>) -> CoreExpr {
        let kind = match &e.kind {
            s::ExprKind::Call { callee, arguments } => {
                if let Some(value) = self.foreign_constructor(callee, arguments, e.span) {
                    value
                } else if let Some(name) = integer_conversion_name(callee) {
                    if arguments.len() != 1 || arguments[0].spread {
                        self.diagnostics.push(conversion_error(
                            e.span,
                            "integer conversion requires exactly one non-spread argument",
                        ));
                        CoreExprKind::Unsupported("invalid integer conversion".into())
                    } else {
                        CoreExprKind::NumericConvert {
                            value: Box::new(self.expr(&arguments[0].value)),
                            target: match name {
                                "u8" => Type::U8,
                                "u16" => Type::U16,
                                "i32" => Type::I32,
                                "i64" => Type::I64,
                                "f32" => Type::F32,
                                "f64" => Type::F64,
                                "u64" => Type::U64,
                                "usize" => Type::Usize,
                                _ => unreachable!(),
                            },
                        }
                    }
                } else if arguments.iter().any(|a| a.spread) {
                    CoreExprKind::Unsupported("tuple argument expansion".into())
                } else {
                    let callee = if let s::ExprKind::TypeApply {
                        callee: target,
                        arguments: types,
                    } = &callee.kind
                    {
                        let static_path = matches!(&target.kind, s::ExprKind::Name(_))
                            || matches!(&target.kind, s::ExprKind::Member { object, member: s::MemberName::Name(_) }
                                if matches!(&object.kind, s::ExprKind::Name(_)));
                        if !static_path
                            || types.len() != 1
                            || !matches!(types.first(), Some(s::GenericArg::Type(_)))
                        {
                            return self.expr_node(CoreExprKind::Unsupported("ordinary type application needs one type and a static direct callee".into()), e.span);
                        }
                        let s::GenericArg::Type(ty) = &types[0] else {
                            unreachable!()
                        };
                        let target = self.expr(target);
                        let argument = self.dyn_type_ref(ty);
                        self.expr_node(
                            CoreExprKind::TypeApply {
                                callee: Box::new(target),
                                type_arguments: vec![argument],
                            },
                            callee.span,
                        )
                    } else {
                        self.expr(callee)
                    };
                    CoreExprKind::Call {
                        callee: Box::new(callee),
                        arguments: arguments.iter().map(|a| self.expr(&a.value)).collect(),
                    }
                }
            }
            _ => unreachable!("lowering expression dispatcher"),
        };
        self.expr_node(kind, e.span)
    }

    #[inline(never)]
    fn lower_expr_member(&mut self, e: &s::Expr, _expected: Option<&Type>) -> CoreExpr {
        let kind = match &e.kind {
            s::ExprKind::Member { object, member } => {
                let (name, owner) = match member {
                    s::MemberName::Name(name) => (name.clone(), None),
                    s::MemberName::TupleIndex(index) => (index.to_string(), None),
                    s::MemberName::Constrained { name, owner } => {
                        let limits = Limits {
                            max_analysis_steps: self
                                .member_owner_limit
                                .saturating_sub(self.callable_initial_work - self.callable_work)
                                .min(self.callable_work)
                                / 3,
                            max_nodes: self.member_owner_nodes,
                            max_call_depth: self.member_owner_depth,
                            ..Limits::default()
                        };
                        let cost = match ct_selection::member_owner_type_cost(
                            owner,
                            self.self_type.as_ref(),
                            &limits,
                        ) {
                            Ok(cost) => cost,
                            Err(errors) => {
                                self.diagnostics.extend(errors);
                                return self.expr_node(CoreExprKind::Unit, e.span);
                            }
                        };
                        // Prepay traversal, lowering and possible compound-place clone.
                        self.callable_work -= cost.work * 3;
                        self.member_owner_work += cost.work * 3;
                        self.member_owner_nodes -= cost.nodes;
                        (name.clone(), Some(self.dyn_type_ref(owner)))
                    }
                };
                CoreExprKind::Member {
                    object: Box::new(self.expr(object)),
                    member: name,
                    owner,
                }
            }
            _ => unreachable!("lowering expression dispatcher"),
        };
        self.expr_node(kind, e.span)
    }

    #[inline(never)]
    fn lower_expr_closure(&mut self, e: &s::Expr, expected: Option<&Type>) -> CoreExpr {
        let kind = match &e.kind {
            s::ExprKind::Closure {
                context: _,
                move_capture,
                params,
                return_type,
                body,
            } => {
                let parameters = params
                    .iter()
                    .map(|p| {
                        self.scoped_binding(&p.name, p.span);
                        CoreClosureParameter {
                            name: p.name.clone(),
                            ty: p.ty.as_ref().map(|t| self.dyn_type_ref(t)),
                            mode: PassingMode::Infer,
                            span: p.span,
                        }
                    })
                    .collect();
                let saved_return = self.return_type.take();
                self.return_type =
                    return_type
                        .as_ref()
                        .map(|t| self.dyn_type_ref(t))
                        .or_else(|| match expected {
                            Some(Type::FunctionPointer { result, .. }) => Some((**result).clone()),
                            _ => None,
                        });
                let closure_return = self.return_type.clone();
                let body = match body {
                    s::ArrowBody::Block(b) => self.block(b),
                    s::ArrowBody::Expression(v) => self.arrow_return(v, closure_return.as_ref()),
                };
                self.return_type = saved_return;
                CoreExprKind::Closure {
                    move_capture: *move_capture,
                    parameters,
                    return_type: closure_return,
                    body,
                }
            }
            _ => unreachable!("lowering expression dispatcher"),
        };
        let lowered = self.expr_node(kind, e.span);
        if let s::ExprKind::Closure { context, .. } = &e.kind
            && let Some(marker) = context.raw_index()
        {
            self.emit_closure_context(marker, lowered.id, e.span);
        }
        lowered
    }

    #[inline(never)]
    fn lower_expr_if(&mut self, e: &s::Expr, expected: Option<&Type>) -> CoreExpr {
        let kind = match &e.kind {
            s::ExprKind::If {
                condition,
                then_value,
                else_value,
            } => CoreExprKind::If {
                condition: Box::new(self.expr(condition)),
                then_value: Box::new(self.expr_expected(then_value, expected)),
                else_value: Box::new(self.expr_expected(else_value, expected)),
            },
            _ => unreachable!("lowering expression dispatcher"),
        };
        self.expr_node(kind, e.span)
    }

    #[inline(never)]
    fn lower_expr_try(&mut self, e: &s::Expr, _expected: Option<&Type>) -> CoreExpr {
        let kind = match &e.kind {
            s::ExprKind::Try(inner) => CoreExprKind::Try(Box::new(self.expr(inner))),
            _ => unreachable!("lowering expression dispatcher"),
        };
        self.expr_node(kind, e.span)
    }

    #[inline(never)]
    fn lower_expr_metaeval(&mut self, e: &s::Expr, _expected: Option<&Type>) -> CoreExpr {
        let kind = match &e.kind {
            s::ExprKind::MetaEval(inner) => CoreExprKind::CompileTime(Box::new(self.expr(inner))),
            _ => unreachable!("lowering expression dispatcher"),
        };
        self.expr_node(kind, e.span)
    }
}
pub fn type_ref(t: &s::TypeRef) -> Type {
    match &t.kind {
        s::TypeKind::Named { path, args } if path.as_slice() == ["Managed"] => {
            if let [s::GenericArg::Type(resource)] = args.as_slice() {
                match type_ref(resource) {
                    Type::Desc(payload)
                        if matches!(
                            payload.as_ref(),
                            Type::ByteBuffer
                                | Type::OwnedStore { .. }
                                | Type::TaskI32
                                | Type::Task { .. }
                                | Type::Registration { .. }
                                | Type::AsyncRegistration { .. }
                        ) =>
                    {
                        Type::Managed(payload)
                    }
                    Type::Managed(_) | Type::Desc(_) => Type::Unsupported(
                        "Managed requires an actual resource, not another carrier".into(),
                    ),
                    resource => Type::Managed(Box::new(resource)),
                }
            } else {
                Type::Unsupported("Managed requires one actual resource type".into())
            }
        }
        s::TypeKind::Named { path, args } if path.as_slice() == ["Task"] => {
            if let [s::GenericArg::Type(result)] = args.as_slice() {
                Type::Desc(Box::new(Type::Task {
                    result: Box::new(type_ref(result)),
                    retained_life: zil_ir::RetainedLifeShape {
                        binder_count: 0,
                        ports: vec![],
                        retained: vec![],
                        nominal_arguments: vec![],
                    },
                }))
            } else {
                Type::Unsupported("Task requires one complete result type".into())
            }
        }
        s::TypeKind::Named { path, args } if matches!(path.as_slice(), [n] if n == "Registration" || n == "AsyncRegistration") => {
            if let [s::GenericArg::Type(environment)] = args.as_slice() {
                Type::Desc(Box::new(if path[0] == "AsyncRegistration" {
                    Type::AsyncRegistration {
                        environment: Box::new(type_ref(environment)),
                    }
                } else {
                    Type::Registration {
                        environment: Box::new(type_ref(environment)),
                    }
                }))
            } else {
                Type::Unsupported("Registration requires one concrete environment type".into())
            }
        }
        s::TypeKind::Named { path, args } if path.as_slice() == ["Store"] => {
            if let [s::GenericArg::Type(element)] = args.as_slice() {
                Type::Desc(Box::new(Type::OwnedStore {
                    element: Box::new(type_ref(element)),
                }))
            } else {
                Type::Unsupported("Store requires exactly one element type".into())
            }
        }
        s::TypeKind::Named { path, args } if path.as_slice() == ["Ref"] && args.len() == 1 => {
            match &args[0] {
                s::GenericArg::Type(inner) => Type::Desc(Box::new(type_ref(inner))),
                _ => Type::Unsupported("Ref requires one resource type".into()),
            }
        }
        s::TypeKind::Named { path, args } if args.is_empty() => match path.join(".").as_str() {
            "u8" => Type::U8,
            "u16" => Type::U16,
            "c_handle" => Type::ForeignHandle,
            "MutexI32" => Type::SyncCell(zil_ir::SyncCellKind::MutexI32),
            "AtomicI32" => Type::SyncCell(zil_ir::SyncCellKind::AtomicI32),
            "c_wstr" => Type::CWideString,
            "i32" => Type::I32,
            "i64" => Type::I64,
            "f32" => Type::F32,
            "f64" => Type::F64,
            "u64" => Type::U64,
            "usize" => Type::Usize,
            "bool" => Type::Bool,
            "void" => Type::Unit,
            n => Type::Named(n.into()),
        },
        s::TypeKind::Union(members) => Type::Enum {
            name: "@anonymous-sum:v1".into(),
            variants: members
                .iter()
                .enumerate()
                .map(|(index, member)| (format!("Member{index}"), vec![type_ref(member)]))
                .collect(),
        },
        s::TypeKind::Tuple(ts) if ts.is_empty() => Type::Unit,
        s::TypeKind::Tuple(ts) => Type::Tuple(ts.iter().map(type_ref).collect()),
        s::TypeKind::Array {
            element,
            length: Some(length),
        } => {
            if let s::ExprKind::Literal(s::Literal::Integer(text)) = &length.kind {
                if let Ok(length) = text.replace('_', "").parse::<u32>() {
                    Type::Array {
                        element: Box::new(type_ref(element)),
                        length,
                    }
                } else {
                    Type::Unsupported("array length must be a bounded integer constant".into())
                }
            } else {
                Type::Unsupported("nonliteral array length needs CT type evaluation".into())
            }
        }
        s::TypeKind::Array {
            element,
            length: None,
        } => Type::Slice {
            element: Box::new(type_ref(element)),
        },
        s::TypeKind::Group(t) => type_ref(t),
        s::TypeKind::Reference {
            lifetime: Some(_), ..
        } => {
            Type::Unsupported("explicit life requires a checked function signature contract".into())
        }
        s::TypeKind::Reference {
            mutable,
            inner,
            lifetime: None,
        } => Type::Address {
            mutable: *mutable,
            inner: Box::new(type_ref(inner)),
        },
        s::TypeKind::Error { error, result } => Type::ErrorUnion {
            error: Box::new(type_ref(error)),
            result: Box::new(type_ref(result)),
        },
        s::TypeKind::ForeignFunction {
            abi,
            params,
            result,
        } if abi == "\"C\"" => Type::ForeignCallback {
            parameters: params.iter().map(type_ref).collect(),
            result: Box::new(type_ref(result)),
        },
        s::TypeKind::Function { params, result } => {
            let parameters: Vec<_> = params.iter().map(type_ref).collect();
            let result = Box::new(type_ref(result));
            if parameters.iter().any(|p| matches!(p, Type::Address { .. }))
                || matches!(result.as_ref(), Type::Address { .. })
            {
                let life = zil_ir::FunctionLifeContract {
                    binder_count: 1,
                    parameter_lives: parameters
                        .iter()
                        .map(|p| matches!(p, Type::Address { .. }).then_some(0))
                        .collect(),
                    result_life: matches!(result.as_ref(), Type::Address { .. }).then_some(0),
                };
                let Some(life) = zil_ir::StorageLifeContract::from_flat(life) else {
                    return Type::Unsupported("callable relation arity exhausted".into());
                };
                Type::BorrowedFunctionPointer {
                    parameters,
                    result,
                    life,
                }
            } else {
                Type::FunctionPointer { parameters, result }
            }
        }
        s::TypeKind::ForLifetime { .. } => lifetimes::pointer_type(t),
        _ => Type::Unsupported("type form is not yet lowered".into()),
    }
}
fn type_path(t: &s::TypeRef) -> Option<String> {
    if let s::TypeKind::Named { path, .. } = &t.kind {
        Some(path.join("."))
    } else {
        None
    }
}
fn expr_path(t: &s::Expr) -> Option<String> {
    match &t.kind {
        s::ExprKind::Name(n) => Some(n.clone()),
        s::ExprKind::Member {
            object,
            member: s::MemberName::Name(n),
        } => Some(format!("{}.{}", expr_path(object)?, n)),
        _ => None,
    }
}

fn mode(p: &s::Param) -> PassingMode {
    if p.give {
        PassingMode::Give
    } else {
        match p.ty.kind {
            s::TypeKind::Reference { mutable: true, .. } => PassingMode::BorrowMut,
            s::TypeKind::Reference { mutable: false, .. } => PassingMode::Borrow,
            _ => PassingMode::Infer,
        }
    }
}
fn binary(op: s::BinaryOp) -> Option<BinaryOp> {
    Some(match op {
        s::BinaryOp::Add => BinaryOp::Add,
        s::BinaryOp::Subtract => BinaryOp::Subtract,
        s::BinaryOp::Multiply => BinaryOp::Multiply,
        s::BinaryOp::Divide => BinaryOp::Divide,
        s::BinaryOp::Remainder => BinaryOp::Remainder,
        s::BinaryOp::AddWrapping => BinaryOp::AddWrapping,
        s::BinaryOp::SubtractWrapping => BinaryOp::SubtractWrapping,
        s::BinaryOp::MultiplyWrapping => BinaryOp::MultiplyWrapping,
        s::BinaryOp::Equal => BinaryOp::Equal,
        s::BinaryOp::NotEqual => BinaryOp::NotEqual,
        s::BinaryOp::Less => BinaryOp::Less,
        s::BinaryOp::LessEqual => BinaryOp::LessEqual,
        s::BinaryOp::Greater => BinaryOp::Greater,
        s::BinaryOp::GreaterEqual => BinaryOp::GreaterEqual,
        s::BinaryOp::And => BinaryOp::And,
        s::BinaryOp::Or => BinaryOp::Or,
        s::BinaryOp::BitAnd => BinaryOp::BitAnd,
        s::BinaryOp::BitOr => BinaryOp::BitOr,
        s::BinaryOp::BitXor => BinaryOp::BitXor,
        s::BinaryOp::ShiftLeft => BinaryOp::ShiftLeft,
        s::BinaryOp::ShiftRight => BinaryOp::ShiftRight,
        s::BinaryOp::Range => return None,
    })
}
