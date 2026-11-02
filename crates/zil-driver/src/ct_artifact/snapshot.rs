//! A separate, immutable pre-generation shape batch. Only ordinary nominal
//! declarations whose type spellings do not depend on CT output enter this
//! batch. Full generated output still runs the ordinary pipeline afterwards.
use std::collections::{BTreeMap, BTreeSet};
use zil_base::{CompileResult, Fingerprint, Limits, ModuleId, SourceUnit, TargetProfile};
use zil_ir::{CoreImport, CoreProgram, ProgramModule, TypedProgram};
use zil_syntax::ast::{self as s, GenericArg, ItemKind, TypeKind};

pub(super) struct ShapeSnapshot {
    pub checked: TypedProgram,
    pub core: CoreProgram,
}

fn plain_type(ty: &s::TypeRef) -> bool {
    match &ty.kind {
        TypeKind::Named { args, .. } | TypeKind::Dyn { args, .. } => args.iter().all(|arg| match arg {
            GenericArg::Type(ty) => plain_type(ty),
            GenericArg::Const(expr) => matches!(expr.kind, s::ExprKind::Literal(_)),
        }),
        TypeKind::Reference { inner, .. } | TypeKind::Group(inner) | TypeKind::Optional(inner) | TypeKind::ForLifetime { inner, .. } => plain_type(inner),
        TypeKind::Tuple(types) | TypeKind::Union(types) | TypeKind::Impl(types) => types.iter().all(plain_type),
        TypeKind::Array { element, length } => plain_type(element) && length.as_ref().is_none_or(|expr| matches!(expr.kind, s::ExprKind::Literal(_))),
        TypeKind::Function { params, result } | TypeKind::ForeignFunction { params, result, .. } => params.iter().all(plain_type) && plain_type(result),
        TypeKind::Error { error, result } => plain_type(error) && plain_type(result),
        TypeKind::MetaEval(_) | TypeKind::MetaBlock(_) | TypeKind::MetaSplice(_) => false,
    }
}

fn plain_item(item: &s::Item) -> Option<s::Item> {
    let mut item = item.clone();
    match &mut item.kind {
        ItemKind::Import(_) => Some(item),
        ItemKind::Aggregate(aggregate) if aggregate.fields.iter().all(|field| plain_type(&field.ty)) => {
            // Defaults and methods do not determine the fields' type shape.
            for field in &mut aggregate.fields { field.default = None; }
            aggregate.methods.clear();
            Some(item)
        }
        ItemKind::Enum(enumeration) if enumeration.variants.iter().all(|variant| match &variant.fields {
            s::VariantFields::Unit => true,
            s::VariantFields::Tuple(fields) => fields.iter().all(|field| plain_type(&field.ty)),
            s::VariantFields::Record(fields) => fields.iter().all(|field| plain_type(&field.ty)),
        }) => Some(item),
        ItemKind::TypeAlias(alias) if alias.value.as_ref().is_some_and(plain_type) => Some(item),
        _ => None,
    }
}

fn name(item: &s::Item) -> Option<&str> {
    match &item.kind {
        ItemKind::Aggregate(a) => Some(&a.name),
        ItemKind::Enum(e) => Some(&e.name),
        ItemKind::TypeAlias(t) => Some(&t.name),
        _ => None,
    }
}

fn referenced_type(ty: &s::TypeRef, names: &mut Vec<Vec<String>>) {
    match &ty.kind {
        TypeKind::Named { path, args } | TypeKind::Dyn { path, args } => {
            names.push(path.clone());
            for arg in args { if let GenericArg::Type(ty) = arg { referenced_type(ty, names); } }
        }
        TypeKind::Reference { inner, .. } | TypeKind::Group(inner) | TypeKind::Optional(inner) | TypeKind::ForLifetime { inner, .. } => referenced_type(inner, names),
        TypeKind::Tuple(types) | TypeKind::Union(types) | TypeKind::Impl(types) => for ty in types { referenced_type(ty, names); },
        TypeKind::Array { element, .. } => referenced_type(element, names),
        TypeKind::Function { params, result } | TypeKind::ForeignFunction { params, result, .. } => { for ty in params { referenced_type(ty, names); } referenced_type(result, names); }
        TypeKind::Error { error, result } => { referenced_type(error, names); referenced_type(result, names); }
        TypeKind::MetaEval(_) | TypeKind::MetaBlock(_) | TypeKind::MetaSplice(_) => {}
    }
}

fn referenced_item(item: &s::Item) -> Vec<Vec<String>> {
    let mut names = Vec::new();
    match &item.kind {
        ItemKind::Aggregate(a) => for field in &a.fields { referenced_type(&field.ty, &mut names); },
        ItemKind::Enum(e) => for variant in &e.variants { match &variant.fields {
            s::VariantFields::Unit => {},
            s::VariantFields::Tuple(fields) => for field in fields { referenced_type(&field.ty, &mut names); },
            s::VariantFields::Record(fields) => for field in fields { referenced_type(&field.ty, &mut names); },
        } },
        ItemKind::TypeAlias(t) => if let Some(ty) = &t.value { referenced_type(ty, &mut names); },
        _ => {}
    }
    names
}

fn builtin(name: &str) -> bool {
    matches!(name, "Unit" | "bool" | "u8" | "u16" | "i32" | "i64" | "ByteBuffer" | "ForeignHandle" | "CWideString" | "Managed" | "Desc" | "Store" | "Slice")
}

pub(super) fn freeze_shapes(
    sources: &[SourceUnit],
    syntax: &[&s::Module],
    imports: &[Vec<CoreImport>],
    root: ModuleId,
    fingerprint: Fingerprint,
    target: &TargetProfile,
    limits: &Limits,
) -> CompileResult<ShapeSnapshot> {
    let mut candidates = BTreeMap::<(ModuleId, String), (usize, usize)>::new();
    for (module_index, module) in syntax.iter().enumerate() {
        for (item_index, item) in module.items.iter().enumerate() {
            if plain_item(item).is_some() && let Some(name) = name(item) {
                candidates.insert((module.id, name.to_owned()), (module_index, item_index));
            }
        }
    }
    let aliases = syntax.iter().enumerate().map(|(index, module)| {
        (module.id, imports[index].iter().map(|edge| (edge.alias.clone(), edge.module)).collect::<BTreeMap<_, _>>())
    }).collect::<BTreeMap<_, _>>();
    // Static dependency pruning examines a finite parsed graph once per edge;
    // it never executes a generator or retries a query into a changed index.
    let mut ready = candidates.keys().cloned().collect::<BTreeSet<_>>();
    loop {
        let before = ready.len();
        let previous = ready.clone();
        ready.retain(|(module, candidate)| {
            let (mi, ii) = candidates[&(*module, candidate.clone())];
            referenced_item(&syntax[mi].items[ii]).iter().all(|path| match path.as_slice() {
                [single] if builtin(single) => true,
                [single] => previous.contains(&(*module, single.clone())),
                [alias, member] => aliases.get(module).and_then(|table| table.get(alias)).is_some_and(|target| previous.contains(&(*target, member.clone()))),
                _ => false,
            })
        });
        if ready.len() == before { break; }
    }
    let selected = syntax.iter().map(|module| s::Module {
        id: module.id,
        span: module.span,
        items: module.items.iter().filter_map(|item| {
            if let Some(name) = name(item) && !ready.contains(&(module.id, name.to_owned())) { return None; }
            plain_item(item)
        }).collect(),
    }).collect::<Vec<_>>();
    let selected = selected.iter().collect::<Vec<_>>();
    let expanded = zil_expand::expand_program_with_limits(sources, &selected, imports, limits)?;
    let modules = expanded.into_iter().map(|(module, _)| ProgramModule { core: module.core, imports: module.imports, exports: module.exports }).collect();
    let core = CoreProgram { root, source_fingerprint: fingerprint, modules };
    // Reuse the language's actual shape resolution and type admission; this
    // batch is never treated as final verification of generated declarations.
    let core = zil_types::resolve_program_shapes(&core, limits)?;
    let checked = zil_types::check_program(&core, target, limits)?;
    Ok(ShapeSnapshot { checked, core })
}
