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
    /// Declared `SyncDrop` bound.
    pub sync_drop: bool,
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
    pub generics: Vec<GenericInfo>,
    pub trait_args: Vec<Ty>,
    pub target: Ty,
    pub assoc_types: Vec<(String, Ty)>,
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
    pub region_params: Vec<String>,
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
    pub noescape_params: Vec<u32>,
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

    /// Matches a (possibly generic) trait impl's target pattern against `self_ty`.
    /// Returns the substituted impl generic arguments if matched.
    pub fn match_impl(&self, imp: &TraitImplDecl, self_ty: Ty, types: &Types) -> Option<Vec<Ty>> {
        let mut bindings: Vec<Option<Ty>> = vec![None; imp.generics.len()];
        if !match_template(imp.target, self_ty, types, &mut bindings) {
            return None;
        }
        bindings.into_iter().collect()
    }

    /// Normalizes associated type projections (`Drain<Item>.Item -> Item`, etc.)
    /// wherever the base type matches a declared trait impl.
    pub fn normalize_ty(&self, ty: Ty, types: &mut Types) -> Ty {
        self.normalize_ty_depth(ty, types, 0)
    }

    fn normalize_ty_depth(&self, ty: Ty, types: &mut Types, depth: u32) -> Ty {
        if depth > 32 {
            return ty;
        }
        let ty = types.shallow(ty);
        match types.data(ty).clone() {
            TyData::Assoc { base, name } => {
                let base = self.normalize_ty_depth(base, types, depth + 1);
                for imp in &self.trait_impls {
                    if let Some((_, rhs)) = imp.assoc_types.iter().find(|(n, _)| *n == name)
                        && let Some(impl_args) = self.match_impl(imp, base, types)
                    {
                        let subst = types.subst(*rhs, &impl_args);
                        return self.normalize_ty_depth(subst, types, depth + 1);
                    }
                }
                types.intern(TyData::Assoc { base, name })
            }
            TyData::Adt { adt, args } => {
                let args = args
                    .into_iter()
                    .map(|a| self.normalize_ty_depth(a, types, depth + 1))
                    .collect();
                types.intern(TyData::Adt { adt, args })
            }
            TyData::Tuple(items) => {
                let items = items
                    .into_iter()
                    .map(|a| self.normalize_ty_depth(a, types, depth + 1))
                    .collect();
                types.intern(TyData::Tuple(items))
            }
            TyData::Array { element, length } => {
                let element = self.normalize_ty_depth(element, types, depth + 1);
                types.intern(TyData::Array { element, length })
            }
            TyData::Slice(element) => {
                let element = self.normalize_ty_depth(element, types, depth + 1);
                types.intern(TyData::Slice(element))
            }
            TyData::Ref {
                region,
                mutable,
                pointee,
            } => {
                let pointee = self.normalize_ty_depth(pointee, types, depth + 1);
                types.intern(TyData::Ref {
                    region,
                    mutable,
                    pointee,
                })
            }
            TyData::Managed(inner) => {
                let inner = self.normalize_ty_depth(inner, types, depth + 1);
                types.intern(TyData::Managed(inner))
            }
            TyData::Desc(inner) => {
                let inner = self.normalize_ty_depth(inner, types, depth + 1);
                types.intern(TyData::Desc(inner))
            }
            TyData::Opaque(OpaqueKind::OwnedStore(elem)) => {
                let elem = self.normalize_ty_depth(elem, types, depth + 1);
                types.intern(TyData::Opaque(OpaqueKind::OwnedStore(elem)))
            }
            TyData::Opaque(OpaqueKind::Mutex(elem)) => {
                let elem = self.normalize_ty_depth(elem, types, depth + 1);
                types.intern(TyData::Opaque(OpaqueKind::Mutex(elem)))
            }
            TyData::Opaque(OpaqueKind::MutexGuard(elem)) => {
                let elem = self.normalize_ty_depth(elem, types, depth + 1);
                types.intern(TyData::Opaque(OpaqueKind::MutexGuard(elem)))
            }
            TyData::Opaque(OpaqueKind::Task { result, context }) => {
                let result = self.normalize_ty_depth(result, types, depth + 1);
                let context = context.map(|c| self.normalize_ty_depth(c, types, depth + 1));
                types.intern(TyData::Opaque(OpaqueKind::Task { result, context }))
            }
            TyData::Opaque(OpaqueKind::AsyncRegistration(elem)) => {
                let elem = self.normalize_ty_depth(elem, types, depth + 1);
                types.intern(TyData::Opaque(OpaqueKind::AsyncRegistration(elem)))
            }
            TyData::Opaque(OpaqueKind::ForeignCallback { params, result }) => {
                let params = params
                    .into_iter()
                    .map(|a| self.normalize_ty_depth(a, types, depth + 1))
                    .collect();
                let result = self.normalize_ty_depth(result, types, depth + 1);
                types.intern(TyData::Opaque(OpaqueKind::ForeignCallback {
                    params,
                    result,
                }))
            }
            TyData::Opaque(OpaqueKind::ByteBuffer | OpaqueKind::ForeignHandle) => ty,
            TyData::ErrorUnion { ok, err } => {
                let ok = self.normalize_ty_depth(ok, types, depth + 1);
                let err = self.normalize_ty_depth(err, types, depth + 1);
                types.intern(TyData::ErrorUnion { ok, err })
            }
            TyData::FnPtr { params, result } => {
                let params = params
                    .into_iter()
                    .map(|a| self.normalize_ty_depth(a, types, depth + 1))
                    .collect();
                let result = self.normalize_ty_depth(result, types, depth + 1);
                types.intern(TyData::FnPtr { params, result })
            }
            TyData::Closure {
                owner,
                id,
                kind,
                params,
                result,
            } => {
                let params = params
                    .into_iter()
                    .map(|a| self.normalize_ty_depth(a, types, depth + 1))
                    .collect();
                let result = self.normalize_ty_depth(result, types, depth + 1);
                types.intern(TyData::Closure {
                    owner,
                    id,
                    kind,
                    params,
                    result,
                })
            }
            TyData::ImplCallable {
                owner,
                kind,
                params,
                result,
            } => {
                let params = params
                    .into_iter()
                    .map(|a| self.normalize_ty_depth(a, types, depth + 1))
                    .collect();
                let result = self.normalize_ty_depth(result, types, depth + 1);
                types.intern(TyData::ImplCallable {
                    owner,
                    kind,
                    params,
                    result,
                })
            }
            TyData::Unit
            | TyData::Bool
            | TyData::Never
            | TyData::Int(_)
            | TyData::Float(_)
            | TyData::Dyn(_)
            | TyData::OwnedDyn(_)
            | TyData::Param(_)
            | TyData::SelfParam
            | TyData::Var(_)
            | TyData::Error => ty,
        }
    }
}

fn match_template(template: Ty, concrete: Ty, types: &Types, bindings: &mut [Option<Ty>]) -> bool {
    let concrete = types.shallow(concrete);
    if let TyData::Param(i) = *types.data(template) {
        let Some(slot) = bindings.get_mut(i as usize) else {
            return false;
        };
        return match *slot {
            None => {
                *slot = Some(concrete);
                true
            }
            Some(prev) => prev == concrete,
        };
    }
    match (types.data(template), types.data(concrete)) {
        (TyData::Adt { adt: a1, args: xs }, TyData::Adt { adt: a2, args: ys }) => {
            a1 == a2
                && xs.len() == ys.len()
                && xs
                    .iter()
                    .zip(ys)
                    .all(|(x, y)| match_template(*x, *y, types, bindings))
        }
        (
            TyData::Ref {
                mutable: m1,
                pointee: p1,
                ..
            },
            TyData::Ref {
                mutable: m2,
                pointee: p2,
                ..
            },
        ) => m1 == m2 && match_template(*p1, *p2, types, bindings),
        (TyData::Tuple(xs), TyData::Tuple(ys)) => {
            xs.len() == ys.len()
                && xs
                    .iter()
                    .zip(ys)
                    .all(|(x, y)| match_template(*x, *y, types, bindings))
        }
        (
            TyData::Array {
                element: e1,
                length: l1,
            },
            TyData::Array {
                element: e2,
                length: l2,
            },
        ) => l1 == l2 && match_template(*e1, *e2, types, bindings),
        (TyData::Slice(e1), TyData::Slice(e2))
        | (TyData::Managed(e1), TyData::Managed(e2))
        | (TyData::Desc(e1), TyData::Desc(e2)) => match_template(*e1, *e2, types, bindings),
        _ => template == concrete,
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
            if parts == ["std", "task", "Poll"] {
                return Some(ItemRef::Adt(AdtRef(2)));
            }
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

    // Built-in prelude ADTs:
    //   AdtRef(0): `enum Option<T> { None, Some(T) }`
    //   AdtRef(1): `enum DynCreateError { Allocation }`
    let option_adt = AdtRef(0);
    items.adt_kinds.push(AdtKind::Enum);
    let option_t = types.intern(TyData::Param(0));
    items.adts.push(AdtDecl {
        name: "Option".to_string(),
        kind: AdtKind::Enum,
        generics: vec!["T".to_string()],
        variants: vec![
            VariantDecl {
                name: "None".to_string(),
                shape: VariantShape::Unit,
                fields: Vec::new(),
            },
            VariantDecl {
                name: "Some".to_string(),
                shape: VariantShape::Tuple,
                fields: vec![FieldDecl {
                    name: "0".to_string(),
                    ty: option_t,
                }],
            },
        ],
        user_drop: false,
    });
    let dyn_create_error_adt = AdtRef(1);
    items.adt_kinds.push(AdtKind::Enum);
    items.adts.push(AdtDecl {
        name: "DynCreateError".to_string(),
        kind: AdtKind::Enum,
        generics: Vec::new(),
        variants: vec![VariantDecl {
            name: "Allocation".to_string(),
            shape: VariantShape::Unit,
            fields: Vec::new(),
        }],
        user_drop: false,
    });
    let poll_adt = AdtRef(2);
    items.adt_kinds.push(AdtKind::Enum);
    items.adts.push(AdtDecl {
        name: "Poll".to_string(),
        kind: AdtKind::Enum,
        generics: vec!["T".to_string()],
        variants: vec![
            VariantDecl {
                name: "Pending".to_string(),
                shape: VariantShape::Unit,
                fields: Vec::new(),
            },
            VariantDecl {
                name: "Ready".to_string(),
                shape: VariantShape::Tuple,
                fields: vec![FieldDecl {
                    name: "0".to_string(),
                    ty: option_t,
                }],
            },
        ],
        user_drop: false,
    });
    let task_create_error_adt = AdtRef(3);
    items.adt_kinds.push(AdtKind::Enum);
    items.adts.push(AdtDecl {
        name: "TaskCreateError".to_string(),
        kind: AdtKind::Enum,
        generics: Vec::new(),
        variants: vec![VariantDecl {
            name: "Allocation".to_string(),
            shape: VariantShape::Unit,
            fields: Vec::new(),
        }],
        user_drop: false,
    });
    let task_poll_error_adt = AdtRef(4);
    items.adt_kinds.push(AdtKind::Enum);
    items.adts.push(AdtDecl {
        name: "TaskPollError".to_string(),
        kind: AdtKind::Enum,
        generics: Vec::new(),
        variants: vec![
            VariantDecl {
                name: "Invalid".to_string(),
                shape: VariantShape::Unit,
                fields: Vec::new(),
            },
            VariantDecl {
                name: "Closed".to_string(),
                shape: VariantShape::Unit,
                fields: Vec::new(),
            },
        ],
        user_drop: false,
    });
    let mutex_create_error_adt = AdtRef(5);
    items.adt_kinds.push(AdtKind::Enum);
    items.adts.push(AdtDecl {
        name: "MutexCreateError".to_string(),
        kind: AdtKind::Enum,
        generics: Vec::new(),
        variants: vec![VariantDecl {
            name: "Allocation".to_string(),
            shape: VariantShape::Unit,
            fields: Vec::new(),
        }],
        user_drop: false,
    });
    let task_create_error_ty = types.intern(TyData::Adt {
        adt: task_create_error_adt,
        args: Vec::new(),
    });
    let _task_create_owned_failure_adt = AdtRef(6);
    items.adt_kinds.push(AdtKind::Package);
    items.adts.push(AdtDecl {
        name: "TaskCreateOwnedFailure".to_string(),
        kind: AdtKind::Package,
        generics: vec!["Inputs".to_string()],
        variants: vec![VariantDecl {
            name: "TaskCreateOwnedFailure".to_string(),
            shape: VariantShape::Record,
            fields: vec![
                FieldDecl {
                    name: "reason".to_string(),
                    ty: task_create_error_ty,
                },
                FieldDecl {
                    name: "inputs".to_string(),
                    ty: option_t,
                },
            ],
        }],
        user_drop: false,
    });

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
                    (
                        &a.name,
                        ItemRef::Adt(AdtRef(items.adt_kinds.len() as u32 - 1)),
                    )
                }
                ast::ItemKind::Enum(e) => {
                    items.adt_kinds.push(AdtKind::Enum);
                    adt_asts.push((mid, AdtAst::Enum(e, item.span)));
                    (
                        &e.name,
                        ItemRef::Adt(AdtRef(items.adt_kinds.len() as u32 - 1)),
                    )
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
        items.modules[m_i]
            .names
            .entry("Option".to_string())
            .or_insert(ItemRef::Adt(option_adt));
        items.modules[m_i]
            .names
            .entry("DynCreateError".to_string())
            .or_insert(ItemRef::Adt(dyn_create_error_adt));
        items.modules[m_i]
            .names
            .entry("Poll".to_string())
            .or_insert(ItemRef::Adt(poll_adt));
        items.modules[m_i]
            .names
            .entry("TaskCreateError".to_string())
            .or_insert(ItemRef::Adt(task_create_error_adt));
        items.modules[m_i]
            .names
            .entry("TaskPollError".to_string())
            .or_insert(ItemRef::Adt(task_poll_error_adt));
        items.modules[m_i]
            .names
            .entry("MutexCreateError".to_string())
            .or_insert(ItemRef::Adt(mutex_create_error_adt));
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
    let b = if let ast::TypeKind::ForLifetime { inner, .. } = &b.kind {
        inner.as_ref()
    } else {
        b
    };
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
) -> (Vec<GenericInfo>, Vec<String>) {
    let scope: Vec<String> = generics
        .iter()
        .filter(|g| matches!(g.kind, ast::GenericParamKind::Type(_)))
        .map(|g| g.name.clone())
        .collect();
    let mut out = Vec::new();
    let mut regions = Vec::new();
    for g in generics {
        match &g.kind {
            ast::GenericParamKind::Type(bounds) => {
                let mut copy = false;
                let mut store_element = false;
                let mut sync_drop = false;
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
                            sync_drop = true;
                        } else if args.is_empty()
                            && matches!(
                                path.as_slice(),
                                [s] if matches!(
                                    s.as_str(),
                                    "Value" | "ClosedEnvironment" | "Transfer" | "ThreadCallable"
                                )
                            )
                        {
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
                    sync_drop,
                    traits,
                    callable,
                });
            }
            ast::GenericParamKind::Lifetime => {
                regions.push(g.name.clone());
            }
            ast::GenericParamKind::Const(_) => {
                diags.push(diag::unsupported("const parameters", g.span));
            }
        }
    }
    (out, regions)
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
    let (generics, _) = type_generics(
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
        match &m.kind {
            ast::ItemKind::TypeAlias(_) => continue,
            ast::ItemKind::Function(f) => {
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
            ast::ItemKind::Import(_)
            | ast::ItemKind::Aggregate(_)
            | ast::ItemKind::Enum(_)
            | ast::ItemKind::Extern { .. }
            | ast::ItemKind::Impl(_)
            | ast::ItemKind::Trait(_)
            | ast::ItemKind::Const(_)
            | ast::ItemKind::Meta(_) => {
                diags.push(diag::unsupported("non-function trait member", m.span));
            }
        }
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
        | TyData::Dyn(_)
        | TyData::OwnedDyn(_)
        | TyData::Param(_)
        | TyData::SelfParam
        | TyData::Assoc { .. }
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
    let (generic_info, _) =
        type_generics(mid, generics, modules, mod_by_path, adt_kinds, types, diags);
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
        } else if c.name == "requires" || c.name == "provides" || c.name == "effects" {
            // Protocol state pre/postconditions and effect bounds (checked in Finalizer/semantics).
            continue;
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
    let (generics, region_params) = type_generics(
        mid,
        &f.generics,
        modules,
        mod_by_path,
        adt_kinds,
        types,
        diags,
    );
    let scope: Vec<String> = generics.iter().map(|g| g.name.clone()).collect();
    let result_mode = parse_result_mode(&f.contract, diags);
    let mut params = Vec::new();
    for p in &f.params {
        let raw_ty = resolve_type_with_lifetimes(
            &p.ty,
            mid,
            &scope,
            &region_params,
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
        Some(t) => resolve_type_with_lifetimes(
            t,
            mid,
            &scope,
            &region_params,
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
        region_params,
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
            | TyData::Opaque(OpaqueKind::ForeignHandle | OpaqueKind::ForeignCallback { .. })
            | TyData::Error
    )
}

fn is_c_abi_noescape_param(ty: Ty, types: &Types) -> bool {
    let TyData::Ref { pointee, .. } = *types.data(ty) else {
        return false;
    };
    if let TyData::Array { element, .. } = *types.data(pointee) {
        is_c_abi_param(element, types)
    } else {
        is_c_abi_param(pointee, types)
    }
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
    let mut noescape_params: Vec<u32> = Vec::new();
    for c in &f.contract {
        if c.name == "provides" {
            for term in &c.terms {
                if term.path == ["noescape"] && term.arguments.len() == 1 {
                    if let [pname] = term.arguments[0].path.as_slice()
                        && let Some(idx) = f.params.iter().position(|p| p.name == *pname)
                    {
                        let idx = idx as u32;
                        if !noescape_params.contains(&idx) {
                            noescape_params.push(idx);
                        }
                    } else {
                        diags.push(diag::error(
                            diag::FFI_DECLARATION,
                            term.span,
                            "noescape must name a parameter of this extern function",
                        ));
                    }
                } else if matches!(
                    term.path.as_slice(),
                    [s] if matches!(
                        s.as_str(),
                        "static_callbacks"
                            | "async_callback_register"
                            | "async_callback_request_close"
                            | "async_callback_poll_drain"
                            | "thread_async_callback_register"
                            | "thread_async_callback_request_close"
                            | "thread_async_callback_poll_drain"
                    )
                ) {
                    continue;
                } else {
                    diags.push(diag::unsupported("extern contract clause", term.span));
                }
            }
        } else if c.name == "effects" {
            for term in &c.terms {
                if term.path != ["ffi"] || !term.arguments.is_empty() {
                    diags.push(diag::unsupported("extern effect clause", term.span));
                }
            }
        } else {
            diags.push(diag::unsupported("extern contract clause", c.span));
        }
    }
    let mut params = Vec::new();
    for (idx, p) in f.params.iter().enumerate() {
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
        let noescape = noescape_params.contains(&(idx as u32));
        let ok = !p.give
            && (is_c_abi_param(ty, types) || (noescape && is_c_abi_noescape_param(ty, types)));
        if !ok {
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
        noescape_params,
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
        region_params: Vec::new(),
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
    let (impl_generics, _) = type_generics(
        mid,
        &imp.generics,
        &items.modules,
        &items.mod_by_path,
        &items.adt_kinds,
        types,
        diags,
    );
    let scope: Vec<String> = impl_generics.iter().map(|g| g.name.clone()).collect();
    let mut trait_args = Vec::new();
    for a in args {
        if let ast::GenericArg::Type(t) = a {
            trait_args.push(
                resolve_type_in(
                    t,
                    mid,
                    &scope,
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
        &scope,
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
    let mut assoc_types = Vec::new();
    let mut methods = Vec::new();
    for m in &imp.members {
        match &m.kind {
            ast::ItemKind::TypeAlias(ta) => {
                if let Some(val) = &ta.value {
                    let ty = resolve_type_in(
                        val,
                        mid,
                        &scope,
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
                    assoc_types.push((ta.name.clone(), ty));
                }
            }
            ast::ItemKind::Function(f) => {
                let mut params = Vec::new();
                if f.params.first().map(|p| p.name.as_str()) != Some("self")
                    && let Some(tm) = items
                        .traits
                        .get(trait_ref.0 as usize)
                        .and_then(|tr| tr.methods.iter().find(|m| m.name == f.name))
                    && tm.params.len() == f.params.len() + 1
                {
                    let self_param_ty = types.subst_with_self(tm.params[0], &trait_args, target);
                    params.push(ParamInfo {
                        name: "self".into(),
                        ty: self_param_ty,
                        span: f.span,
                    });
                }
                for p in &f.params {
                    let ty = resolve_type_in(
                        &p.ty,
                        mid,
                        &scope,
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
                        &scope,
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
                    generics: impl_generics.clone(),
                    region_params: Vec::new(),
                    params,
                    result,
                    role: FnRoleSpec::Normal,
                    ast: f,
                });
                methods.push((f.name.clone(), fn_ref));
            }
            ast::ItemKind::Import(_)
            | ast::ItemKind::Aggregate(_)
            | ast::ItemKind::Enum(_)
            | ast::ItemKind::Extern { .. }
            | ast::ItemKind::Impl(_)
            | ast::ItemKind::Trait(_)
            | ast::ItemKind::Const(_)
            | ast::ItemKind::Meta(_) => {
                diags.push(diag::unsupported("non-function impl member", m.span));
            }
        }
    }
    items.trait_impls.push(TraitImplDecl {
        trait_ref,
        generics: impl_generics,
        trait_args,
        target,
        assoc_types,
        methods,
    });
}

/// Resolves a written type in the scope of module `from`, `generics`, and `lifetimes`.
#[allow(clippy::result_large_err)] // `Diagnostic` is the workspace-wide error type.
pub fn resolve_type(
    t: &ast::TypeRef,
    from: ModIdx,
    generics: &[String],
    lifetimes: &[String],
    items: &Items<'_>,
    types: &mut Types,
) -> Result<Ty, Diagnostic> {
    resolve_type_with_lifetimes(
        t,
        from,
        generics,
        lifetimes,
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
    resolve_type_with_lifetimes(
        t,
        from,
        generics,
        &[],
        modules,
        mod_by_path,
        adt_kinds,
        self_ty,
        owner_fn,
        types,
    )
}

#[allow(clippy::result_large_err, clippy::too_many_arguments)]
fn resolve_type_with_lifetimes(
    t: &ast::TypeRef,
    from: ModIdx,
    generics: &[String],
    lifetimes: &[String],
    modules: &[ModuleScope],
    mod_by_path: &HashMap<String, ModIdx>,
    adt_kinds: &[AdtKind],
    self_ty: Option<Ty>,
    owner_fn: Option<FnRef>,
    types: &mut Types,
) -> Result<Ty, Diagnostic> {
    match &t.kind {
        ast::TypeKind::Named { path, args } => {
            if let [base, assoc] = path.as_slice()
                && args.is_empty()
            {
                if base == "Self"
                    && let Some(st) = self_ty
                {
                    return Ok(types.intern(TyData::Assoc {
                        base: st,
                        name: assoc.clone(),
                    }));
                }
                if let Some(i) = generics.iter().position(|g| g == base) {
                    let base_ty = types.intern(TyData::Param(i as u32));
                    return Ok(types.intern(TyData::Assoc {
                        base: base_ty,
                        name: assoc.clone(),
                    }));
                }
            }
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
                    let elem = resolve_type_with_lifetimes(
                        elem,
                        from,
                        generics,
                        lifetimes,
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
                if name == "OwnedDyn" {
                    let [ast::GenericArg::Type(inner_ast)] = args.as_slice() else {
                        return Err(diag::error(
                            diag::TYPE_ARGUMENT,
                            t.span,
                            "`OwnedDyn<dyn Trait>` takes one type argument",
                        ));
                    };
                    let inner = resolve_type_with_lifetimes(
                        inner_ast,
                        from,
                        generics,
                        lifetimes,
                        modules,
                        mod_by_path,
                        adt_kinds,
                        self_ty,
                        None,
                        types,
                    )?;
                    let TyData::Dyn(tr) = *types.data(inner) else {
                        return Err(diag::error(
                            diag::TYPE_ARGUMENT,
                            t.span,
                            "`OwnedDyn` argument must be a `dyn Trait` type",
                        ));
                    };
                    return Ok(types.intern(TyData::OwnedDyn(tr)));
                }
                if matches!(
                    name.as_str(),
                    "Task" | "Mutex" | "MutexGuard" | "AsyncRegistration"
                ) {
                    let (elem_ast, ctx_ty) = match (name.as_str(), args.as_slice()) {
                        ("Task", [ast::GenericArg::Type(r), ast::GenericArg::Type(ctx)]) => {
                            let c = resolve_type_with_lifetimes(
                                ctx,
                                from,
                                generics,
                                lifetimes,
                                modules,
                                mod_by_path,
                                adt_kinds,
                                self_ty,
                                None,
                                types,
                            )?;
                            (r, Some(c))
                        }
                        (_, [ast::GenericArg::Type(elem)]) => (elem, None),
                        _ => {
                            return Err(diag::error(
                                diag::TYPE_ARGUMENT,
                                t.span,
                                format!("`{name}<T>` takes one type argument"),
                            ));
                        }
                    };
                    let elem = resolve_type_with_lifetimes(
                        elem_ast,
                        from,
                        generics,
                        lifetimes,
                        modules,
                        mod_by_path,
                        adt_kinds,
                        self_ty,
                        None,
                        types,
                    )?;
                    return match name.as_str() {
                        "Task" => {
                            let inner = types.intern(TyData::Opaque(OpaqueKind::Task {
                                result: elem,
                                context: ctx_ty,
                            }));
                            Ok(types.intern(TyData::Desc(inner)))
                        }
                        "Mutex" => {
                            let inner = types.intern(TyData::Opaque(OpaqueKind::Mutex(elem)));
                            Ok(types.intern(TyData::Desc(inner)))
                        }
                        "MutexGuard" => {
                            Ok(types.intern(TyData::Opaque(OpaqueKind::MutexGuard(elem))))
                        }
                        _ => {
                            let inner =
                                types.intern(TyData::Opaque(OpaqueKind::AsyncRegistration(elem)));
                            Ok(types.intern(TyData::Desc(inner)))
                        }
                    };
                }
                if name == "Managed" {
                    let [ast::GenericArg::Type(inner_ast)] = args.as_slice() else {
                        return Err(diag::error(
                            diag::TYPE_ARGUMENT,
                            t.span,
                            "`Managed<R>` takes one type argument",
                        ));
                    };
                    let resolved = resolve_type_with_lifetimes(
                        inner_ast,
                        from,
                        generics,
                        lifetimes,
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
                            ast::GenericArg::Type(t) => tys.push(resolve_type_with_lifetimes(
                                t,
                                from,
                                generics,
                                lifetimes,
                                modules,
                                mod_by_path,
                                adt_kinds,
                                self_ty,
                                None,
                                types,
                            )?),
                            ast::GenericArg::Lifetime(_) => {}
                            ast::GenericArg::Const(_) | ast::GenericArg::AssociatedBinding(_) => {
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
                tys.push(resolve_type_with_lifetimes(
                    i,
                    from,
                    generics,
                    lifetimes,
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
        ast::TypeKind::Group(inner) => resolve_type_with_lifetimes(
            inner,
            from,
            generics,
            lifetimes,
            modules,
            mod_by_path,
            adt_kinds,
            self_ty,
            None,
            types,
        ),
        ast::TypeKind::Array { element, length } => {
            let element = resolve_type_with_lifetimes(
                element,
                from,
                generics,
                lifetimes,
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
            let mut pointee = resolve_type_with_lifetimes(
                inner,
                from,
                generics,
                lifetimes,
                modules,
                mod_by_path,
                adt_kinds,
                self_ty,
                None,
                types,
            )?;
            if *mutable
                && let TyData::Desc(inner_ty) = *types.data(pointee)
                && matches!(
                    types.data(inner_ty),
                    TyData::Opaque(OpaqueKind::Task { .. } | OpaqueKind::AsyncRegistration(_))
                )
            {
                pointee = types.intern(TyData::Managed(inner_ty));
            }
            let region = lifetime.as_deref().and_then(|lt| {
                let name = lt.strip_prefix('\'').unwrap_or(lt);
                lifetimes.iter().position(|r| r == name).map(|i| i as u32)
            });
            Ok(types.intern(TyData::Ref {
                region,
                mutable: *mutable,
                pointee,
            }))
        }
        ast::TypeKind::Function { params, result } => {
            let mut ps = Vec::with_capacity(params.len());
            for p in params {
                ps.push(resolve_type_with_lifetimes(
                    p,
                    from,
                    generics,
                    lifetimes,
                    modules,
                    mod_by_path,
                    adt_kinds,
                    self_ty,
                    None,
                    types,
                )?);
            }
            let r = resolve_type_with_lifetimes(
                result,
                from,
                generics,
                lifetimes,
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
            let err = resolve_type_with_lifetimes(
                error,
                from,
                generics,
                lifetimes,
                modules,
                mod_by_path,
                adt_kinds,
                self_ty,
                None,
                types,
            )?;
            let ok = resolve_type_with_lifetimes(
                result,
                from,
                generics,
                lifetimes,
                modules,
                mod_by_path,
                adt_kinds,
                self_ty,
                None,
                types,
            )?;
            Ok(types.intern(TyData::ErrorUnion { ok, err }))
        }
        ast::TypeKind::ForeignFunction {
            abi,
            params,
            result,
        } => {
            if abi.trim_matches('"') != "C" {
                return Err(diag::error(
                    diag::FFI_DECLARATION,
                    t.span,
                    format!("unsupported extern ABI `{abi}`; only \"C\" is supported"),
                ));
            }
            let mut ps = Vec::with_capacity(params.len());
            for p in params {
                ps.push(resolve_type_with_lifetimes(
                    p,
                    from,
                    generics,
                    lifetimes,
                    modules,
                    mod_by_path,
                    adt_kinds,
                    self_ty,
                    None,
                    types,
                )?);
            }
            let r = resolve_type_with_lifetimes(
                result,
                from,
                generics,
                lifetimes,
                modules,
                mod_by_path,
                adt_kinds,
                self_ty,
                None,
                types,
            )?;
            Ok(types.intern(TyData::Opaque(OpaqueKind::ForeignCallback {
                params: ps,
                result: r,
            })))
        }
        ast::TypeKind::Optional(inner) => {
            let elem = resolve_type_with_lifetimes(
                inner,
                from,
                generics,
                lifetimes,
                modules,
                mod_by_path,
                adt_kinds,
                self_ty,
                None,
                types,
            )?;
            Ok(types.intern(TyData::Adt {
                adt: AdtRef(0),
                args: vec![elem],
            }))
        }
        ast::TypeKind::Union(_) => Err(diag::unsupported("anonymous sum types", t.span)),
        ast::TypeKind::Dyn { path, .. } => {
            if let Some(ItemRef::Trait(tr)) = lookup_item_in(modules, mod_by_path, from, path) {
                Ok(types.intern(TyData::Dyn(tr)))
            } else {
                Err(diag::error(
                    diag::NAME_UNRESOLVED,
                    t.span,
                    format!("unknown trait `{}` in `dyn` type", path.join(".")),
                ))
            }
        }
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
