//! resolve + collect: the item table of a single- or multi-module program
//! and the semantic form of every declaration.
//!
//! Declarations become templates over their own type parameters
//! (`TyData::Param`). Field identity is "declaration + variant + ordinal";
//! names are kept for diagnostics and source resolution only.

use crate::ModuleInput;
use crate::diag;
use crate::thir::{FnRef, ForeignRef};
use crate::ty::{AdtRef, OpaqueKind, TraitRef, Ty, TyData, Types};
use std::collections::{HashMap, HashSet};
use zil_base::{Diagnostic, Span};
use zil_ir::body::{AdtKind, ClosureKind, FloatTy, IntTy};
use zil_syntax::ast;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ModIdx(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemRef {
    Fn(FnRef),
    Adt(AdtRef),
    Foreign(ForeignRef),
    Trait(TraitRef),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FnRoleSpec {
    Normal,
    Drop(AdtRef),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResultMode {
    Default,
    Create,
    Give,
}

#[derive(Clone, Debug)]
pub struct GenericInfo {
    pub name: String,
    /// Declared `Copy` bound.
    pub copy: bool,
    /// Declared `StoreElement` bound.
    pub store_element: bool,
    /// Declared user trait bounds: `(trait_ref, trait_args)`.
    pub traits: Vec<(TraitRef, Vec<Ty>)>,
    /// Declared `Fn`/`FnMut`/`FnOnce` bound: `(kind, params, result)`.
    pub callable: Option<(ClosureKind, Vec<Ty>, Ty)>,
}

#[derive(Clone, Debug)]
pub struct TraitMethodDecl {
    pub name: String,
    pub params: Vec<Ty>,
    pub result: Ty,
}

#[derive(Clone, Debug)]
pub struct TraitDecl {
    pub name: String,
    pub methods: Vec<TraitMethodDecl>,
}

#[derive(Clone, Debug)]
pub struct TraitImplDecl {
    pub trait_ref: TraitRef,
    pub trait_args: Vec<Ty>,
    pub target: Ty,
    pub methods: Vec<(String, FnRef)>,
}

#[derive(Clone, Debug)]
pub struct ParamInfo {
    pub name: String,
    pub ty: Ty,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct FnDecl<'a> {
    pub module: ModIdx,
    /// Canonical qualified name (or bare name for an unnamed root module).
    pub name: String,
    pub generics: Vec<GenericInfo>,
    pub params: Vec<ParamInfo>,
    pub result: Ty,
    pub role: FnRoleSpec,
    pub ast: &'a ast::Function,
}

#[derive(Clone, Debug)]
pub struct ForeignDeclInfo {
    pub name: String,
    pub symbol: String,
    pub params: Vec<Ty>,
    pub result: Ty,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VariantShape {
    Unit,
    Tuple,
    Record,
}

#[derive(Clone, Debug)]
pub struct FieldDecl {
    pub name: String,
    pub ty: Ty,
}

#[derive(Clone, Debug)]
pub struct VariantDecl {
    pub name: String,
    pub shape: VariantShape,
    pub fields: Vec<FieldDecl>,
}

#[derive(Clone, Debug)]
pub struct AdtDecl {
    pub name: String,
    pub kind: AdtKind,
    pub generics: Vec<String>,
    pub variants: Vec<VariantDecl>,
    pub user_drop: bool,
}

impl AdtDecl {
    pub fn variant_named(&self, name: &str) -> Option<u32> {
        self.variants
            .iter()
            .position(|v| v.name == name)
            .map(|i| i as u32)
    }
}

#[derive(Default)]
pub struct ModuleScope {
    pub path: String,
    pub names: HashMap<String, ItemRef>,
    pub publics: HashSet<String>,
    pub imports: HashMap<String, ModIdx>,
}

pub struct Items<'a> {
    pub modules: Vec<ModuleScope>,
    pub mod_by_path: HashMap<String, ModIdx>,
    pub fns: Vec<FnDecl<'a>>,
    pub adts: Vec<AdtDecl>,
    pub foreigns: Vec<ForeignDeclInfo>,
    pub traits: Vec<TraitDecl>,
    pub trait_impls: Vec<TraitImplDecl>,
    pub adt_kinds: Vec<AdtKind>,
}

impl<'a> Items<'a> {
    pub fn adt(&self, adt: AdtRef) -> &AdtDecl {
        &self.adts[adt.0 as usize]
    }
    pub fn func(&self, f: FnRef) -> &FnDecl<'a> {
        &self.fns[f.0 as usize]
    }
    pub fn foreign(&self, f: ForeignRef) -> &ForeignDeclInfo {
        &self.foreigns[f.0 as usize]
    }
    pub fn trait_decl(&self, t: TraitRef) -> &TraitDecl {
        &self.traits[t.0 as usize]
    }

    /// Resolves a bare or module-qualified item path from module `from`.
    pub fn lookup_item(&self, from: ModIdx, path: &[String]) -> Option<ItemRef> {
        lookup_item_in(&self.modules, &self.mod_by_path, from, path)
    }
}

fn lookup_item_in(
    modules: &[ModuleScope],
    mod_by_path: &HashMap<String, ModIdx>,
    from: ModIdx,
    path: &[String],
) -> Option<ItemRef> {
    match path {
        [name] => modules[from.0 as usize].names.get(name).copied(),
        [alias, name] => {
            let target = modules[from.0 as usize]
                .imports
                .get(alias)
                .copied()
                .or_else(|| mod_by_path.get(alias).copied())?;
            let scope = &modules[target.0 as usize];
            if target != from && !scope.publics.contains(name) {
                return None;
            }
            scope.names.get(name).copied()
        }
        parts if parts.len() >= 3 => {
            let mod_path = parts[..parts.len() - 1].join(".");
            let name = parts.last()?;
            let target = *mod_by_path.get(&mod_path)?;
            let scope = &modules[target.0 as usize];
            if target != from && !scope.publics.contains(name) {
                return None;
            }
            scope.names.get(name).copied()
        }
        _ => None,
    }
}

pub fn scalar_type(name: &str) -> Option<TyData> {
    Some(match name {
        "i8" => TyData::Int(IntTy::I8),
        "i16" => TyData::Int(IntTy::I16),
        "i32" => TyData::Int(IntTy::I32),
        "i64" => TyData::Int(IntTy::I64),
        "isize" => TyData::Int(IntTy::Isize),
        "u8" => TyData::Int(IntTy::U8),
        "u16" => TyData::Int(IntTy::U16),
        "u32" => TyData::Int(IntTy::U32),
        "u64" => TyData::Int(IntTy::U64),
        "usize" => TyData::Int(IntTy::Usize),
        "f32" => TyData::Float(FloatTy::F32),
        "f64" => TyData::Float(FloatTy::F64),
        "bool" => TyData::Bool,
        "void" => TyData::Unit,
        "c_handle" => TyData::Opaque(OpaqueKind::ForeignHandle),
        _ => return None,
    })
}

fn qualify(mod_path: &str, name: &str) -> String {
    if mod_path.is_empty() {
        name.to_string()
    } else {
        format!("{mod_path}.{name}")
    }
}

/// Builds the item table of a program (one or more modules) and collects
/// every declaration.
pub fn collect_program<'a>(
    inputs: &[ModuleInput<'a>],
    types: &mut Types,
    diags: &mut Vec<Diagnostic>,
) -> Items<'a> {
    let mut items = Items {
        modules: Vec::with_capacity(inputs.len()),
        mod_by_path: HashMap::new(),
        fns: Vec::new(),
        adts: Vec::new(),
        foreigns: Vec::new(),
        traits: Vec::new(),
        trait_impls: Vec::new(),
        adt_kinds: Vec::new(),
    };
    for (i, inp) in inputs.iter().enumerate() {
        let idx = ModIdx(i as u32);
        items.modules.push(ModuleScope {
            path: inp.path.clone(),
            names: HashMap::new(),
            publics: HashSet::new(),
            imports: HashMap::new(),
        });
        if !inp.path.is_empty() {
            items.mod_by_path.insert(inp.path.clone(), idx);
        }
    }

    // Pass 1: top-level names, ADT kinds and module imports.
    let mut adt_asts: Vec<(ModIdx, AdtAst<'a>)> = Vec::new();
    let mut fn_asts: Vec<(ModIdx, &'a ast::Function)> = Vec::new();
    let mut foreign_asts: Vec<(ModIdx, &str, &'a ast::Function, Span)> = Vec::new();
    let mut impl_asts: Vec<(ModIdx, &'a ast::ImplBlock, Span)> = Vec::new();
    let mut trait_asts: Vec<(ModIdx, &'a ast::TraitDecl, Span)> = Vec::new();

    for (m_i, inp) in inputs.iter().enumerate() {
        let mid = ModIdx(m_i as u32);
        for item in &inp.ast.items {
            let (name, r) = match &item.kind {
                ast::ItemKind::Import(imp) => {
                    if imp.syntax {
                        diags.push(diag::unsupported("syntax imports", item.span));
                        continue;
                    }
                    let target_path = imp.path.join(".");
                    let Some(&target_mod) = items.mod_by_path.get(&target_path) else {
                        diags.push(diag::error(
                            diag::NAME_UNRESOLVED,
                            item.span,
                            format!("unknown module `{target_path}`"),
                        ));
                        continue;
                    };
                    let alias = imp
                        .alias
                        .clone()
                        .or_else(|| imp.path.last().cloned())
                        .unwrap_or_default();
                    items.modules[m_i].imports.insert(alias, target_mod);
                    continue;
                }
                ast::ItemKind::Function(f) => {
                    fn_asts.push((mid, f));
                    (&f.name, ItemRef::Fn(FnRef(fn_asts.len() as u32 - 1)))
                }
                ast::ItemKind::Aggregate(a) => {
                    let kind = match a.kind {
                        ast::AggregateKind::Struct => AdtKind::Record,
                        ast::AggregateKind::Package => AdtKind::Package,
                        ast::AggregateKind::Resource => AdtKind::Resource,
                    };
                    items.adt_kinds.push(kind);
                    adt_asts.push((mid, AdtAst::Aggregate(a, item.span)));
                    (&a.name, ItemRef::Adt(AdtRef(adt_asts.len() as u32 - 1)))
                }
                ast::ItemKind::Enum(e) => {
                    items.adt_kinds.push(AdtKind::Enum);
                    adt_asts.push((mid, AdtAst::Enum(e, item.span)));
                    (&e.name, ItemRef::Adt(AdtRef(adt_asts.len() as u32 - 1)))
                }
                ast::ItemKind::Extern { abi, function } => {
                    foreign_asts.push((mid, abi.as_str(), function, item.span));
                    (
                        &function.name,
                        ItemRef::Foreign(ForeignRef(foreign_asts.len() as u32 - 1)),
                    )
                }
                ast::ItemKind::Impl(imp) => {
                    impl_asts.push((mid, imp, item.span));
                    continue;
                }
                ast::ItemKind::Trait(tr) => {
                    trait_asts.push((mid, tr, item.span));
                    (
                        &tr.name,
                        ItemRef::Trait(TraitRef(trait_asts.len() as u32 - 1)),
                    )
                }
                ast::ItemKind::TypeAlias(_) => {
                    diags.push(diag::unsupported("type aliases", item.span));
                    continue;
                }
                ast::ItemKind::Const(_) => {
                    diags.push(diag::unsupported("constant items", item.span));
                    continue;
                }
                ast::ItemKind::Meta(_) => {
                    diags.push(diag::unsupported("meta items", item.span));
                    continue;
                }
            };
            if items.modules[m_i].names.insert(name.clone(), r).is_some() {
                diags.push(diag::error(
                    diag::NAME_DUPLICATE,
                    item.span,
                    format!("`{name}` is declared more than once"),
                ));
            }
            if item.public {
                items.modules[m_i].publics.insert(name.clone());
            }
        }
    }

    // Pass 2: declarations. Collect traits first so function/ADT generic bounds can reference them.
    for (mid, tr, span) in trait_asts {
        let decl = collect_trait(
            mid,
            tr,
            span,
            &items.modules,
            &items.mod_by_path,
            &items.adt_kinds,
            types,
            diags,
        );
        items.traits.push(decl);
    }
    for (mid, item) in adt_asts {
        let decl = collect_adt(
            mid,
            item,
            &items.modules,
            &items.mod_by_path,
            &items.adt_kinds,
            types,
            diags,
        );
        items.adts.push(decl);
    }
    for (fi, (mid, f)) in fn_asts.into_iter().enumerate() {
        let decl = collect_fn(
            mid,
            FnRef(fi as u32),
            f,
            &items.modules,
            &items.mod_by_path,
            &items.adt_kinds,
            types,
            diags,
        );
        items.fns.push(decl);
    }
    for (mid, abi, f, span) in foreign_asts {
        let decl = collect_foreign(
            mid,
            abi,
            f,
            span,
            &items.modules,
            &items.mod_by_path,
            &items.adt_kinds,
            types,
            diags,
        );
        items.foreigns.push(decl);
    }
    for (mid, imp, span) in impl_asts {
        collect_impl(mid, imp, span, &mut items, types, diags);
    }
    items
}

#[allow(clippy::too_many_arguments)]
fn resolve_callable_bound(
    b: &ast::TypeRef,
    mid: ModIdx,
    scope: &[String],
    modules: &[ModuleScope],
    mod_by_path: &HashMap<String, ModIdx>,
    adt_kinds: &[AdtKind],
    types: &mut Types,
    diags: &mut Vec<Diagnostic>,
) -> Option<(ClosureKind, Vec<Ty>, Ty)> {
    let ast::TypeKind::Named { path, args } = &b.kind else {
        return None;
    };
    let [name] = path.as_slice() else {
        return None;
    };
    let kind = match name.as_str() {
        "Fn" => ClosureKind::Fn,
        "FnMut" => ClosureKind::FnMut,
        "FnOnce" => ClosureKind::FnOnce,
        _ => return None,
    };
    let [
        ast::GenericArg::Type(params_ty),
        ast::GenericArg::Type(ret_ty),
    ] = args.as_slice()
    else {
        diags.push(diag::error(
            diag::TYPE_ARGUMENT,
            b.span,
            "callable bound requires parameter list and return type",
        ));
        return Some((kind, Vec::new(), types.error()));
    };
    let mut params = Vec::new();
    if let ast::TypeKind::Tuple(items) = &params_ty.kind {
        for p in items {
            params.push(
                resolve_type_in(
                    p,
                    mid,
                    scope,
                    modules,
                    mod_by_path,
                    adt_kinds,
                    None,
                    None,
                    types,
                )
                .unwrap_or_else(|d| {
                    diags.push(d);
                    types.error()
                }),
            );
        }
    } else {
        params.push(
            resolve_type_in(
                params_ty,
                mid,
                scope,
                modules,
                mod_by_path,
                adt_kinds,
                None,
                None,
                types,
            )
            .unwrap_or_else(|d| {
                diags.push(d);
                types.error()
            }),
        );
    }
    let result = resolve_type_in(
        ret_ty,
        mid,
        scope,
        modules,
        mod_by_path,
        adt_kinds,
        None,
        None,
        types,
    )
    .unwrap_or_else(|d| {
        diags.push(d);
        types.error()
    });
    Some((kind, params, result))
}

#[allow(clippy::too_many_arguments)]
fn type_generics(
    mid: ModIdx,
    generics: &[ast::GenericParam],
    modules: &[ModuleScope],
    mod_by_path: &HashMap<String, ModIdx>,
    adt_kinds: &[AdtKind],
    types: &mut Types,
    diags: &mut Vec<Diagnostic>,
) -> Vec<GenericInfo> {
    let scope: Vec<String> = generics.iter().map(|g| g.name.clone()).collect();
    let mut out = Vec::new();
    for g in generics {
        match &g.kind {
            ast::GenericParamKind::Type(bounds) => {
                let mut copy = false;
                let mut store_element = false;
                let mut traits = Vec::new();
                let mut callable = None;
                for b in bounds {
                    if let Some(c) = resolve_callable_bound(
                        b,
                        mid,
                        &scope,
                        modules,
                        mod_by_path,
                        adt_kinds,
                        types,
                        diags,
                    ) {
                        callable = Some(c);
                        continue;
                    }
                    if let ast::TypeKind::Named { path, args } = &b.kind {
                        if args.is_empty() && path == &["Copy"] {
                            copy = true;
                        } else if args.is_empty() && path == &["StoreElement"] {
                            store_element = true;
                        } else if args.is_empty() && path == &["SyncDrop"] {
                            // SyncDrop bound accepted; checked against async/non-sync types when those arrive.
                            continue;
                        } else if let Some(ItemRef::Trait(tr)) =
                            lookup_item_in(modules, mod_by_path, mid, path)
                        {
                            let mut targs = Vec::new();
                            for a in args {
                                if let ast::GenericArg::Type(t) = a {
                                    targs.push(
                                        resolve_type_in(
                                            t,
                                            mid,
                                            &scope,
                                            modules,
                                            mod_by_path,
                                            adt_kinds,
                                            None,
                                            None,
                                            types,
                                        )
                                        .unwrap_or_else(
                                            |d| {
                                                diags.push(d);
                                                types.error()
                                            },
                                        ),
                                    );
                                } else {
                                    diags.push(diag::unsupported(
                                        "non-type trait arguments",
                                        b.span,
                                    ));
                                }
                            }
                            traits.push((tr, targs));
                        } else {
                            diags.push(diag::unsupported("unknown trait bound", b.span));
                        }
                    } else {
                        diags.push(diag::unsupported("non-named trait bound", b.span));
                    }
                }
                out.push(GenericInfo {
                    name: g.name.clone(),
                    copy,
                    store_element,
                    traits,
                    callable,
                });
            }
            ast::GenericParamKind::Lifetime => {
                diags.push(diag::unsupported("lifetime parameters", g.span));
            }
            ast::GenericParamKind::Const(_) => {
                diags.push(diag::unsupported("const parameters", g.span));
            }
        }
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn collect_trait(
    mid: ModIdx,
    tr: &ast::TraitDecl,
    _span: Span,
    modules: &[ModuleScope],
    mod_by_path: &HashMap<String, ModIdx>,
    adt_kinds: &[AdtKind],
    types: &mut Types,
    diags: &mut Vec<Diagnostic>,
) -> TraitDecl {
    let generics = type_generics(
        mid,
        &tr.generics,
        modules,
        mod_by_path,
        adt_kinds,
        types,
        diags,
    );
    let scope: Vec<String> = generics.iter().map(|g| g.name.clone()).collect();
    let self_param = types.intern(TyData::SelfParam);
    let mut methods = Vec::new();
    for m in &tr.members {
        let ast::ItemKind::Function(f) = &m.kind else {
            diags.push(diag::unsupported("non-function trait member", m.span));
            continue;
        };
        let mut params = Vec::new();
        for p in &f.params {
            let ty = resolve_type_in(
                &p.ty,
                mid,
                &scope,
                modules,
                mod_by_path,
                adt_kinds,
                Some(self_param),
                None,
                types,
            )
            .unwrap_or_else(|d| {
                diags.push(d);
                types.error()
            });
            params.push(ty);
        }
        let result = match &f.return_type {
            Some(t) => resolve_type_in(
                t,
                mid,
                &scope,
                modules,
                mod_by_path,
                adt_kinds,
                Some(self_param),
                None,
                types,
            )
            .unwrap_or_else(|d| {
                diags.push(d);
                types.error()
            }),
            None => types.unit(),
        };
        methods.push(TraitMethodDecl {
            name: f.name.clone(),
            params,
            result,
        });
    }
    TraitDecl {
        name: qualify(&modules[mid.0 as usize].path, &tr.name),
        methods,
    }
}

#[derive(Clone, Copy)]
enum AdtAst<'a> {
    Aggregate(&'a ast::Aggregate, Span),
    Enum(&'a ast::EnumDecl, Span),
}

fn promote_to_managed(ty: Ty, types: &mut Types) -> Option<Ty> {
    match types.data(ty).clone() {
        TyData::Desc(inner) | TyData::Managed(inner) => Some(types.intern(TyData::Managed(inner))),
        TyData::Error => Some(ty),
        TyData::Unit
        | TyData::Bool
        | TyData::Never
        | TyData::Int(_)
        | TyData::Float(_)
        | TyData::Adt { .. }
        | TyData::Tuple(_)
        | TyData::Array { .. }
        | TyData::Slice(_)
        | TyData::Ref { .. }
        | TyData::Opaque(_)
        | TyData::ErrorUnion { .. }
        | TyData::FnPtr { .. }
        | TyData::Closure { .. }
        | TyData::ImplCallable { .. }
        | TyData::Param(_)
        | TyData::SelfParam
        | TyData::Var(_) => None,
    }
}

fn promote_carrier_or_error_ok(ty: Ty, types: &mut Types) -> Option<Ty> {
    if let Some(m) = promote_to_managed(ty, types) {
        return Some(m);
    }
    if let TyData::ErrorUnion { ok, err } = types.data(ty).clone()
        && let Some(m) = promote_to_managed(ok, types)
    {
        return Some(types.intern(TyData::ErrorUnion { ok: m, err }));
    }
    None
}

fn collect_adt(
    mid: ModIdx,
    item: AdtAst<'_>,
    modules: &[ModuleScope],
    mod_by_path: &HashMap<String, ModIdx>,
    adt_kinds: &[AdtKind],
    types: &mut Types,
    diags: &mut Vec<Diagnostic>,
) -> AdtDecl {
    let (name, kind, generics, span) = match item {
        AdtAst::Aggregate(a, span) => {
            let kind = match a.kind {
                ast::AggregateKind::Struct => AdtKind::Record,
                ast::AggregateKind::Package => AdtKind::Package,
                ast::AggregateKind::Resource => AdtKind::Resource,
            };
            if !a.methods.is_empty() {
                diags.push(diag::unsupported("methods", span));
            }
            (&a.name, kind, &a.generics, span)
        }
        AdtAst::Enum(e, span) => (&e.name, AdtKind::Enum, &e.generics, span),
    };
    let generic_info = type_generics(mid, generics, modules, mod_by_path, adt_kinds, types, diags);
    let scope: Vec<String> = generic_info.iter().map(|g| g.name.clone()).collect();
    let mut resolve = |t: &ast::TypeRef, diags: &mut Vec<Diagnostic>, types: &mut Types| {
        resolve_type_in(
            t,
            mid,
            &scope,
            modules,
            mod_by_path,
            adt_kinds,
            None,
            None,
            types,
        )
        .unwrap_or_else(|d| {
            diags.push(d);
            types.error()
        })
    };
    let fields_of =
        |fields: &[ast::Field],
         owner_kind: AdtKind,
         diags: &mut Vec<Diagnostic>,
         types: &mut Types,
         resolve: &mut dyn FnMut(&ast::TypeRef, &mut Vec<Diagnostic>, &mut Types) -> Ty|
         -> Vec<FieldDecl> {
            let mut out: Vec<FieldDecl> = Vec::new();
            for f in fields {
                if out.iter().any(|o| o.name == f.name) {
                    diags.push(diag::error(
                        "E-FIELD-DUPLICATE",
                        f.span,
                        format!("field `{}` is declared more than once", f.name),
                    ));
                }
                if f.default.is_some() {
                    diags.push(diag::unsupported("field defaults", f.span));
                }
                let raw_ty = resolve(&f.ty, diags, types);
                let ty = if f.manages {
                    if owner_kind != AdtKind::Package {
                        diags.push(diag::error(
                            diag::RESOURCE_TYPE,
                            f.span,
                            "`manages` fields are only allowed in a `package`",
                        ));
                        types.error()
                    } else if let Some(managed) = promote_to_managed(raw_ty, types) {
                        managed
                    } else {
                        diags.push(diag::error(
                            diag::RESOURCE_TYPE,
                            f.span,
                            "`manages` field must have a resource, `Store<T>` or `ByteBuffer` type",
                        ));
                        types.error()
                    }
                } else {
                    if let TyData::Desc(inner) = types.data(raw_ty)
                        && matches!(
                            types.data(*inner),
                            TyData::Opaque(OpaqueKind::ByteBuffer | OpaqueKind::OwnedStore(_))
                        )
                    {
                        diags.push(diag::error(
                            diag::RESOURCE_TYPE,
                            f.span,
                            "Store and ByteBuffer fields require `manages`",
                        ));
                    }
                    raw_ty
                };
                out.push(FieldDecl {
                    name: f.name.clone(),
                    ty,
                });
            }
            out
        };
    let variants = match item {
        AdtAst::Aggregate(a, _) => vec![VariantDecl {
            name: a.name.clone(),
            shape: VariantShape::Record,
            fields: fields_of(&a.fields, kind, diags, types, &mut resolve),
        }],
        AdtAst::Enum(e, _) => {
            let mut vs: Vec<VariantDecl> = Vec::new();
            for v in &e.variants {
                if vs.iter().any(|o| o.name == v.name) {
                    diags.push(diag::error(
                        "E-ENUM-VARIANT-DUPLICATE",
                        v.span,
                        format!("variant `{}` is declared more than once", v.name),
                    ));
                }
                let (shape, fields) = match &v.fields {
                    ast::VariantFields::Unit => (VariantShape::Unit, Vec::new()),
                    ast::VariantFields::Tuple(items) => {
                        let mut fs = Vec::new();
                        for (i, f) in items.iter().enumerate() {
                            if f.manages {
                                diags.push(diag::unsupported("managed enum fields", f.span));
                            }
                            fs.push(FieldDecl {
                                name: i.to_string(),
                                ty: resolve(&f.ty, diags, types),
                            });
                        }
                        (VariantShape::Tuple, fs)
                    }
                    ast::VariantFields::Record(fields) => (
                        VariantShape::Record,
                        fields_of(fields, kind, diags, types, &mut resolve),
                    ),
                };
                vs.push(VariantDecl {
                    name: v.name.clone(),
                    shape,
                    fields,
                });
            }
            vs
        }
    };
    let _ = span;
    AdtDecl {
        name: qualify(&modules[mid.0 as usize].path, name),
        kind,
        generics: scope,
        variants,
        user_drop: false,
    }
}

fn parse_result_mode(clauses: &[ast::ContractClause], diags: &mut Vec<Diagnostic>) -> ResultMode {
    let mut mode = ResultMode::Default;
    for c in clauses {
        if c.name == "result" {
            for term in &c.terms {
                let next = match term.path.as_slice() {
                    [s] if s == "create" && term.arguments.is_empty() => Some(ResultMode::Create),
                    [s] if s == "give" && term.arguments.is_empty() => Some(ResultMode::Give),
                    _ => None,
                };
                match next {
                    Some(m) if mode == ResultMode::Default || mode == m => mode = m,
                    Some(_) => diags.push(diag::error(
                        diag::RESOURCE_RESULT_MODE,
                        term.span,
                        "`result(create)` and `result(give)` are mutually exclusive",
                    )),
                    None => diags.push(diag::unsupported("function contracts", term.span)),
                }
            }
        } else {
            diags.push(diag::unsupported("function contracts", c.span));
        }
    }
    mode
}

#[allow(clippy::too_many_arguments)]
fn collect_fn<'a>(
    mid: ModIdx,
    fn_ref: FnRef,
    f: &'a ast::Function,
    modules: &[ModuleScope],
    mod_by_path: &HashMap<String, ModIdx>,
    adt_kinds: &[AdtKind],
    types: &mut Types,
    diags: &mut Vec<Diagnostic>,
) -> FnDecl<'a> {
    let generics = type_generics(
        mid,
        &f.generics,
        modules,
        mod_by_path,
        adt_kinds,
        types,
        diags,
    );
    let scope: Vec<String> = generics.iter().map(|g| g.name.clone()).collect();
    if !f.where_clause.is_empty() {
        diags.push(diag::unsupported("where clauses", f.span));
    }
    let result_mode = parse_result_mode(&f.contract, diags);
    let mut params = Vec::new();
    for p in &f.params {
        let raw_ty = resolve_type_in(
            &p.ty,
            mid,
            &scope,
            modules,
            mod_by_path,
            adt_kinds,
            None,
            None,
            types,
        )
        .unwrap_or_else(|d| {
            diags.push(d);
            types.error()
        });
        let ty = if p.give {
            if let Some(m) = promote_carrier_or_error_ok(raw_ty, types) {
                m
            } else {
                diags.push(diag::error(
                    diag::GIVE_TYPE,
                    p.span,
                    "`give` parameter must have a managed resource or `E!Managed` type",
                ));
                types.error()
            }
        } else {
            raw_ty
        };
        params.push(ParamInfo {
            name: p.name.clone(),
            ty,
            span: p.span,
        });
    }
    let raw_result = match &f.return_type {
        Some(t) => resolve_type_in(
            t,
            mid,
            &scope,
            modules,
            mod_by_path,
            adt_kinds,
            None,
            Some(fn_ref),
            types,
        )
        .unwrap_or_else(|d| {
            diags.push(d);
            types.error()
        }),
        None => types.unit(),
    };
    let result = match result_mode {
        ResultMode::Default => raw_result,
        ResultMode::Create | ResultMode::Give => {
            if let Some(m) = promote_carrier_or_error_ok(raw_result, types) {
                m
            } else {
                diags.push(diag::error(
                    diag::RESOURCE_RESULT_MODE,
                    f.span,
                    "`result(create)`/`result(give)` requires a managed resource or `E!Managed` return type",
                ));
                types.error()
            }
        }
    };
    FnDecl {
        module: mid,
        name: qualify(&modules[mid.0 as usize].path, &f.name),
        generics,
        params,
        result,
        role: FnRoleSpec::Normal,
        ast: f,
    }
}

fn is_c_abi_param(ty: Ty, types: &Types) -> bool {
    matches!(
        types.data(ty),
        TyData::Int(_)
            | TyData::Float(_)
            | TyData::Opaque(OpaqueKind::ForeignHandle)
            | TyData::Error
    )
}

#[allow(clippy::too_many_arguments)]
fn collect_foreign(
    mid: ModIdx,
    abi: &str,
    f: &ast::Function,
    span: Span,
    modules: &[ModuleScope],
    mod_by_path: &HashMap<String, ModIdx>,
    adt_kinds: &[AdtKind],
    types: &mut Types,
    diags: &mut Vec<Diagnostic>,
) -> ForeignDeclInfo {
    if abi.trim_matches('"') != "C" {
        diags.push(diag::error(
            diag::FFI_DECLARATION,
            span,
            format!("unsupported extern ABI `{abi}`; only \"C\" is supported"),
        ));
    }
    if !f.generics.is_empty() || !f.where_clause.is_empty() {
        diags.push(diag::error(
            diag::FFI_DECLARATION,
            span,
            "extern \"C\" functions cannot be generic",
        ));
    }
    if !matches!(f.body, ast::FunctionBody::Declaration) {
        diags.push(diag::error(
            diag::FFI_DECLARATION,
            span,
            "extern \"C\" functions cannot have a body",
        ));
    }
    let mut params = Vec::new();
    for p in &f.params {
        let ty = resolve_type_in(
            &p.ty,
            mid,
            &[],
            modules,
            mod_by_path,
            adt_kinds,
            None,
            None,
            types,
        )
        .unwrap_or_else(|d| {
            diags.push(d);
            types.error()
        });
        if p.give || !is_c_abi_param(ty, types) {
            diags.push(diag::error(
                diag::FFI_DECLARATION,
                p.span,
                "extern \"C\" parameter type is outside the C value ABI",
            ));
        }
        params.push(ty);
    }
    let result = match &f.return_type {
        Some(t) => resolve_type_in(
            t,
            mid,
            &[],
            modules,
            mod_by_path,
            adt_kinds,
            None,
            None,
            types,
        )
        .unwrap_or_else(|d| {
            diags.push(d);
            types.error()
        }),
        None => types.unit(),
    };
    if !matches!(types.data(result), TyData::Unit) && !is_c_abi_param(result, types) {
        diags.push(diag::error(
            diag::FFI_DECLARATION,
            span,
            "extern \"C\" return type is outside the C value ABI",
        ));
    }
    ForeignDeclInfo {
        name: f.name.clone(),
        symbol: f.name.clone(),
        params,
        result,
        span,
    }
}

fn collect_impl<'a>(
    mid: ModIdx,
    imp: &'a ast::ImplBlock,
    span: Span,
    items: &mut Items<'a>,
    types: &mut Types,
    diags: &mut Vec<Diagnostic>,
) {
    let is_drop = matches!(
        &imp.trait_type,
        Some(t) if matches!(&t.kind, ast::TypeKind::Named { path, args } if path == &["Drop"] && args.is_empty())
    );
    if !is_drop {
        collect_trait_impl(mid, imp, span, items, types, diags);
        return;
    }
    if !imp.generics.is_empty() || !imp.where_clause.is_empty() || imp.receiver_default.is_some() {
        diags.push(diag::error(
            diag::DROP_BINDING,
            span,
            "`impl Drop` cannot declare generics, where clauses or receiver defaults",
        ));
        return;
    }
    let ast::TypeKind::Named { path, args } = &imp.target.kind else {
        diags.push(diag::error(
            diag::DROP_BINDING,
            imp.target.span,
            "`impl Drop` target must be a local `resource` name",
        ));
        return;
    };
    if path.len() != 1 || !args.is_empty() {
        diags.push(diag::error(
            diag::DROP_BINDING,
            imp.target.span,
            "`impl Drop` target must be a local `resource` name",
        ));
        return;
    }
    let target_adt = match items.modules[mid.0 as usize].names.get(&path[0]) {
        Some(ItemRef::Adt(adt)) if items.adts[adt.0 as usize].kind == AdtKind::Resource => {
            if !items.adts[adt.0 as usize].generics.is_empty() {
                diags.push(diag::error(
                    diag::DROP_BINDING,
                    imp.target.span,
                    "`impl Drop` target resource cannot be generic",
                ));
                return;
            }
            *adt
        }
        Some(ItemRef::Adt(_) | ItemRef::Fn(_) | ItemRef::Foreign(_) | ItemRef::Trait(_)) | None => {
            diags.push(diag::error(
                diag::DROP_BINDING,
                imp.target.span,
                "`impl Drop` is only allowed for a local `resource` type",
            ));
            return;
        }
    };
    if items.adts[target_adt.0 as usize].user_drop {
        diags.push(diag::error(
            diag::DROP_DUPLICATE,
            span,
            format!(
                "duplicate `impl Drop` for `{}`",
                items.adts[target_adt.0 as usize].name
            ),
        ));
        return;
    }
    let [member] = imp.members.as_slice() else {
        diags.push(diag::error(
            diag::DROP_SIGNATURE,
            span,
            "`impl Drop` must contain exactly `fn drop(self: Self)`",
        ));
        return;
    };
    let ast::ItemKind::Function(f) = &member.kind else {
        diags.push(diag::error(
            diag::DROP_SIGNATURE,
            member.span,
            "`impl Drop` may only contain `fn drop(self: Self)`",
        ));
        return;
    };
    if f.name != "drop"
        || !f.generics.is_empty()
        || !f.where_clause.is_empty()
        || !f.contract.is_empty()
        || f.params.len() != 1
    {
        diags.push(diag::error(
            diag::DROP_SIGNATURE,
            f.span,
            "`impl Drop` entry must be `fn drop(self: Self)`",
        ));
        return;
    }
    let payload_ty = types.intern(TyData::Adt {
        adt: target_adt,
        args: Vec::new(),
    });
    let self_desc = types.intern(TyData::Desc(payload_ty));
    let p = &f.params[0];
    let p_ty = resolve_type_in(
        &p.ty,
        mid,
        &[],
        &items.modules,
        &items.mod_by_path,
        &items.adt_kinds,
        Some(self_desc),
        None,
        types,
    )
    .unwrap_or_else(|d| {
        diags.push(d);
        types.error()
    });
    let ret_ty = match &f.return_type {
        Some(t) => resolve_type_in(
            t,
            mid,
            &[],
            &items.modules,
            &items.mod_by_path,
            &items.adt_kinds,
            Some(self_desc),
            None,
            types,
        )
        .unwrap_or_else(|d| {
            diags.push(d);
            types.error()
        }),
        None => types.unit(),
    };
    if p.name != "self"
        || p.give
        || p_ty != self_desc
        || !matches!(types.data(ret_ty), TyData::Unit)
    {
        diags.push(diag::error(
            diag::DROP_SIGNATURE,
            f.span,
            "`impl Drop` entry must be `fn drop(self: Self)` returning unit",
        ));
        return;
    }
    items.adts[target_adt.0 as usize].user_drop = true;
    let drop_name = format!("{}.$drop", items.adts[target_adt.0 as usize].name);
    items.fns.push(FnDecl {
        module: mid,
        name: drop_name,
        generics: Vec::new(),
        params: vec![ParamInfo {
            name: p.name.clone(),
            ty: self_desc,
            span: p.span,
        }],
        result: types.unit(),
        role: FnRoleSpec::Drop(target_adt),
        ast: f,
    });
}

fn collect_trait_impl<'a>(
    mid: ModIdx,
    imp: &'a ast::ImplBlock,
    span: Span,
    items: &mut Items<'a>,
    types: &mut Types,
    diags: &mut Vec<Diagnostic>,
) {
    let Some(trait_ty) = &imp.trait_type else {
        diags.push(diag::unsupported("inherent impl blocks", span));
        return;
    };
    let ast::TypeKind::Named { path, args } = &trait_ty.kind else {
        diags.push(diag::unsupported("non-named trait impl", trait_ty.span));
        return;
    };
    let Some(ItemRef::Trait(trait_ref)) =
        lookup_item_in(&items.modules, &items.mod_by_path, mid, path)
    else {
        diags.push(diag::error(
            diag::NAME_UNRESOLVED,
            trait_ty.span,
            format!("unknown trait `{}`", path.join(".")),
        ));
        return;
    };
    if !imp.generics.is_empty() || !imp.where_clause.is_empty() {
        diags.push(diag::unsupported("generic trait impls", span));
        return;
    }
    let mut trait_args = Vec::new();
    for a in args {
        if let ast::GenericArg::Type(t) = a {
            trait_args.push(
                resolve_type_in(
                    t,
                    mid,
                    &[],
                    &items.modules,
                    &items.mod_by_path,
                    &items.adt_kinds,
                    None,
                    None,
                    types,
                )
                .unwrap_or_else(|d| {
                    diags.push(d);
                    types.error()
                }),
            );
        } else {
            diags.push(diag::unsupported("non-type trait arguments", trait_ty.span));
        }
    }
    let target = resolve_type_in(
        &imp.target,
        mid,
        &[],
        &items.modules,
        &items.mod_by_path,
        &items.adt_kinds,
        None,
        None,
        types,
    )
    .unwrap_or_else(|d| {
        diags.push(d);
        types.error()
    });
    let trait_name = items.traits[trait_ref.0 as usize].name.clone();
    let mut methods = Vec::new();
    for m in &imp.members {
        let ast::ItemKind::Function(f) = &m.kind else {
            diags.push(diag::unsupported("non-function impl member", m.span));
            continue;
        };
        let mut params = Vec::new();
        for p in &f.params {
            let ty = resolve_type_in(
                &p.ty,
                mid,
                &[],
                &items.modules,
                &items.mod_by_path,
                &items.adt_kinds,
                Some(target),
                None,
                types,
            )
            .unwrap_or_else(|d| {
                diags.push(d);
                types.error()
            });
            params.push(ParamInfo {
                name: p.name.clone(),
                ty,
                span: p.span,
            });
        }
        let result = match &f.return_type {
            Some(t) => resolve_type_in(
                t,
                mid,
                &[],
                &items.modules,
                &items.mod_by_path,
                &items.adt_kinds,
                Some(target),
                None,
                types,
            )
            .unwrap_or_else(|d| {
                diags.push(d);
                types.error()
            }),
            None => types.unit(),
        };
        let fn_ref = FnRef(items.fns.len() as u32);
        items.fns.push(FnDecl {
            module: mid,
            name: format!("{trait_name}::{}", f.name),
            generics: Vec::new(),
            params,
            result,
            role: FnRoleSpec::Normal,
            ast: f,
        });
        methods.push((f.name.clone(), fn_ref));
    }
    items.trait_impls.push(TraitImplDecl {
        trait_ref,
        trait_args,
        target,
        methods,
    });
}

/// Resolves a written type in the scope of module `from` and `generics`.
#[allow(clippy::result_large_err)] // `Diagnostic` is the workspace-wide error type.
pub fn resolve_type(
    t: &ast::TypeRef,
    from: ModIdx,
    generics: &[String],
    items: &Items<'_>,
    types: &mut Types,
) -> Result<Ty, Diagnostic> {
    resolve_type_in(
        t,
        from,
        generics,
        &items.modules,
        &items.mod_by_path,
        &items.adt_kinds,
        None,
        None,
        types,
    )
}

#[allow(clippy::result_large_err, clippy::too_many_arguments)]
fn resolve_type_in(
    t: &ast::TypeRef,
    from: ModIdx,
    generics: &[String],
    modules: &[ModuleScope],
    mod_by_path: &HashMap<String, ModIdx>,
    adt_kinds: &[AdtKind],
    self_ty: Option<Ty>,
    owner_fn: Option<FnRef>,
    types: &mut Types,
) -> Result<Ty, Diagnostic> {
    match &t.kind {
        ast::TypeKind::Named { path, args } => {
            if let [name] = path.as_slice() {
                if name == "Self"
                    && let Some(st) = self_ty
                {
                    if !args.is_empty() {
                        return Err(diag::error(
                            diag::TYPE_ARGUMENT,
                            t.span,
                            "`Self` takes no type arguments",
                        ));
                    }
                    return Ok(st);
                }
                if let Some(i) = generics.iter().position(|g| g == name) {
                    if !args.is_empty() {
                        return Err(diag::error(
                            diag::TYPE_ARGUMENT,
                            t.span,
                            "a type parameter takes no arguments",
                        ));
                    }
                    return Ok(types.intern(TyData::Param(i as u32)));
                }
                if let Some(s) = scalar_type(name) {
                    if !args.is_empty() {
                        return Err(diag::error(
                            diag::TYPE_ARGUMENT,
                            t.span,
                            "a scalar type takes no arguments",
                        ));
                    }
                    return Ok(types.intern(s));
                }
                if name == "ByteBuffer" {
                    if !args.is_empty() {
                        return Err(diag::error(
                            diag::TYPE_ARGUMENT,
                            t.span,
                            "`ByteBuffer` takes no type arguments",
                        ));
                    }
                    let inner = types.intern(TyData::Opaque(OpaqueKind::ByteBuffer));
                    return Ok(types.intern(TyData::Desc(inner)));
                }
                if name == "Store" {
                    let [ast::GenericArg::Type(elem)] = args.as_slice() else {
                        return Err(diag::error(
                            diag::TYPE_ARGUMENT,
                            t.span,
                            "`Store<T>` takes one type argument",
                        ));
                    };
                    let elem = resolve_type_in(
                        elem,
                        from,
                        generics,
                        modules,
                        mod_by_path,
                        adt_kinds,
                        self_ty,
                        None,
                        types,
                    )?;
                    let inner = types.intern(TyData::Opaque(OpaqueKind::OwnedStore(elem)));
                    return Ok(types.intern(TyData::Desc(inner)));
                }
                if name == "Managed" {
                    let [ast::GenericArg::Type(inner_ast)] = args.as_slice() else {
                        return Err(diag::error(
                            diag::TYPE_ARGUMENT,
                            t.span,
                            "`Managed<R>` takes one type argument",
                        ));
                    };
                    let resolved = resolve_type_in(
                        inner_ast,
                        from,
                        generics,
                        modules,
                        mod_by_path,
                        adt_kinds,
                        self_ty,
                        None,
                        types,
                    )?;
                    return if let TyData::Desc(inner) = types.data(resolved).clone() {
                        Ok(types.intern(TyData::Managed(inner)))
                    } else {
                        Err(diag::error(
                            diag::RESOURCE_TYPE,
                            t.span,
                            "`Managed<R>` requires a resource, `Store<T>` or `ByteBuffer` type",
                        ))
                    };
                }
            }
            match lookup_item_in(modules, mod_by_path, from, path) {
                Some(ItemRef::Adt(adt)) => {
                    let mut tys = Vec::new();
                    for a in args {
                        match a {
                            ast::GenericArg::Type(t) => tys.push(resolve_type_in(
                                t,
                                from,
                                generics,
                                modules,
                                mod_by_path,
                                adt_kinds,
                                self_ty,
                                None,
                                types,
                            )?),
                            ast::GenericArg::Lifetime(_)
                            | ast::GenericArg::Const(_)
                            | ast::GenericArg::AssociatedBinding(_) => {
                                return Err(diag::unsupported(
                                    "non-type generic arguments",
                                    t.span,
                                ));
                            }
                        }
                    }
                    let adt_ty = types.intern(TyData::Adt { adt, args: tys });
                    if adt_kinds[adt.0 as usize] == AdtKind::Resource {
                        Ok(types.intern(TyData::Desc(adt_ty)))
                    } else {
                        Ok(adt_ty)
                    }
                }
                Some(ItemRef::Fn(_) | ItemRef::Foreign(_) | ItemRef::Trait(_)) | None => {
                    Err(diag::error(
                        diag::NAME_UNRESOLVED,
                        t.span,
                        format!("unknown type `{}`", path.join(".")),
                    ))
                }
            }
        }
        ast::TypeKind::Tuple(items) => {
            if items.is_empty() {
                return Ok(types.unit());
            }
            let mut tys = Vec::new();
            for i in items {
                tys.push(resolve_type_in(
                    i,
                    from,
                    generics,
                    modules,
                    mod_by_path,
                    adt_kinds,
                    self_ty,
                    None,
                    types,
                )?);
            }
            Ok(types.intern(TyData::Tuple(tys)))
        }
        ast::TypeKind::Group(inner) => resolve_type_in(
            inner,
            from,
            generics,
            modules,
            mod_by_path,
            adt_kinds,
            self_ty,
            None,
            types,
        ),
        ast::TypeKind::Array { element, length } => {
            let element = resolve_type_in(
                element,
                from,
                generics,
                modules,
                mod_by_path,
                adt_kinds,
                self_ty,
                None,
                types,
            )?;
            let Some(length) = length else {
                return Ok(types.intern(TyData::Slice(element)));
            };
            let n = if let ast::ExprKind::Literal(ast::Literal::Integer(text)) = &length.kind {
                crate::literal::parse_int(text).and_then(|(v, suffix)| match suffix {
                    None | Some(IntTy::Usize) => u32::try_from(v).ok(),
                    Some(_) => None,
                })
            } else {
                None
            };
            let Some(length) = n else {
                return Err(diag::error(
                    diag::ARRAY_LENGTH,
                    t.span,
                    "array length must be a usize literal",
                ));
            };
            Ok(types.intern(TyData::Array { element, length }))
        }
        ast::TypeKind::Reference {
            lifetime,
            mutable,
            inner,
        } => {
            if lifetime.is_some() {
                // Written regions arrive with region contracts (P4).
                return Err(diag::unsupported("explicit lifetimes", t.span));
            }
            let pointee = resolve_type_in(
                inner,
                from,
                generics,
                modules,
                mod_by_path,
                adt_kinds,
                self_ty,
                None,
                types,
            )?;
            Ok(types.intern(TyData::Ref {
                mutable: *mutable,
                pointee,
            }))
        }
        ast::TypeKind::Function { params, result } => {
            let mut ps = Vec::with_capacity(params.len());
            for p in params {
                ps.push(resolve_type_in(
                    p,
                    from,
                    generics,
                    modules,
                    mod_by_path,
                    adt_kinds,
                    self_ty,
                    None,
                    types,
                )?);
            }
            let r = resolve_type_in(
                result,
                from,
                generics,
                modules,
                mod_by_path,
                adt_kinds,
                self_ty,
                None,
                types,
            )?;
            Ok(types.intern(TyData::FnPtr {
                params: ps,
                result: r,
            }))
        }
        ast::TypeKind::Error { error, result } => {
            let err = resolve_type_in(
                error,
                from,
                generics,
                modules,
                mod_by_path,
                adt_kinds,
                self_ty,
                None,
                types,
            )?;
            let ok = resolve_type_in(
                result,
                from,
                generics,
                modules,
                mod_by_path,
                adt_kinds,
                self_ty,
                None,
                types,
            )?;
            Ok(types.intern(TyData::ErrorUnion { ok, err }))
        }
        ast::TypeKind::ForeignFunction { .. } => {
            Err(diag::unsupported("foreign function types", t.span))
        }
        ast::TypeKind::Optional(_) => Err(diag::unsupported("optional types", t.span)),
        ast::TypeKind::Union(_) => Err(diag::unsupported("anonymous sum types", t.span)),
        ast::TypeKind::Dyn { .. } => Err(diag::unsupported("dyn types", t.span)),
        ast::TypeKind::ForLifetime { .. } => {
            Err(diag::unsupported("higher-ranked lifetimes", t.span))
        }
        ast::TypeKind::Impl(bounds) => {
            let Some(owner) = owner_fn else {
                return Err(diag::unsupported(
                    "impl types outside return position",
                    t.span,
                ));
            };
            let [bound] = bounds.as_slice() else {
                return Err(diag::unsupported("multi-bound impl return types", t.span));
            };
            let mut diags = Vec::new();
            let Some((kind, params, result)) = resolve_callable_bound(
                bound,
                from,
                generics,
                modules,
                mod_by_path,
                adt_kinds,
                types,
                &mut diags,
            ) else {
                return Err(diag::unsupported("non-callable impl return types", t.span));
            };
            if let Some(d) = diags.into_iter().next() {
                return Err(d);
            }
            Ok(types.intern(TyData::ImplCallable {
                owner,
                kind,
                params,
                result,
            }))
        }
        ast::TypeKind::MetaEval(_) | ast::TypeKind::MetaSplice(_) | ast::TypeKind::MetaBlock(_) => {
            Err(diag::unsupported("compile-time types", t.span))
        }
    }
}
