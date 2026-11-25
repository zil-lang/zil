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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ConstRef(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemRef {
    Fn(FnRef),
    Adt(AdtRef),
    Foreign(ForeignRef),
    Trait(TraitRef),
    TypeAlias(ModIdx, u32),
    Const(ConstRef),
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraitBound {
    pub trait_ref: TraitRef,
    pub trait_args: Vec<Ty>,
    /// Associated type equality bindings, e.g. `Item = i64` in `Container<Item = i64>`.
    pub assoc_bindings: Vec<(String, Ty)>,
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
    /// Declared user trait bounds.
    pub traits: Vec<TraitBound>,
    /// Declared `Fn`/`FnMut`/`FnOnce` bound: `(kind, params, result)`.
    pub callable: Option<(ClosureKind, Vec<Ty>, Ty)>,
    /// If `Some(ty)`, this is a `const` generic parameter of integer type `ty`.
    pub const_ty: Option<Ty>,
}

#[derive(Clone, Debug)]
pub struct TraitMethodDecl<'a> {
    pub name: String,
    pub generics: Vec<GenericInfo>,
    pub region_params: Vec<String>,
    pub params: Vec<Ty>,
    pub result: Ty,
    pub default_ast: Option<&'a ast::Function>,
}

#[derive(Clone, Debug)]
pub struct TraitDecl<'a> {
    pub module: ModIdx,
    pub name: String,
    pub generics: Vec<GenericInfo>,
    pub supertraits: Vec<TraitBound>,
    pub assoc_types: Vec<String>,
    pub methods: Vec<TraitMethodDecl<'a>>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct TraitImplDecl {
    pub trait_ref: TraitRef,
    pub generics: Vec<GenericInfo>,
    pub where_bounds: Vec<(Ty, TraitBound)>,
    pub trait_args: Vec<Ty>,
    pub target: Ty,
    pub assoc_types: Vec<(String, Ty)>,
    pub methods: Vec<(String, FnRef)>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct InherentMethodDecl {
    pub name: String,
    pub fn_ref: FnRef,
    pub public: bool,
    pub has_self: bool,
    pub impl_generic_count: usize,
    pub module: ModIdx,
}

#[derive(Clone, Debug)]
pub struct InherentImplDecl {
    pub generics: Vec<GenericInfo>,
    pub target: Ty,
    pub methods: Vec<InherentMethodDecl>,
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
    pub self_ty: Option<Ty>,
    pub role: FnRoleSpec,
    pub resume_mode: zil_ir::body::ResumeMode,
    pub state_contracts: Vec<zil_ir::body::StateContractClause>,
    pub ast: &'a ast::Function,
}

#[derive(Clone, Debug)]
pub struct InlineMethodDecl {
    pub name: String,
    pub fn_ref: FnRef,
    pub public: bool,
    pub has_self: bool,
    pub host_generic_count: usize,
}

#[derive(Clone, Debug)]
pub struct ForeignDeclInfo {
    pub name: String,
    pub symbol: String,
    pub params: Vec<Ty>,
    pub noescape_params: Vec<u32>,
    pub callback_role: Option<zil_ir::body::ForeignCallbackRole>,
    pub thread_async_driver_transfer: bool,
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
    pub default: Option<ast::Expr>,
    pub public: bool,
}

#[derive(Clone, Debug)]
pub struct VariantDecl {
    pub name: String,
    pub shape: VariantShape,
    pub fields: Vec<FieldDecl>,
}

#[derive(Clone, Debug)]
pub struct AdtDecl {
    pub module: ModIdx,
    pub name: String,
    pub kind: AdtKind,
    pub generics: Vec<String>,
    pub region_params: u32,
    pub variants: Vec<VariantDecl>,
    pub methods: Vec<InlineMethodDecl>,
    pub user_drop: bool,
}

impl AdtDecl {
    pub fn variant_named(&self, name: &str) -> Option<u32> {
        self.variants
            .iter()
            .position(|v| v.name == name)
            .map(|i| i as u32)
    }

    pub fn method_named(&self, name: &str) -> Option<&InlineMethodDecl> {
        self.methods.iter().find(|m| m.name == name)
    }
}

#[derive(Default)]
pub struct ModuleScope {
    pub path: String,
    pub names: HashMap<String, ItemRef>,
    pub publics: HashSet<String>,
    pub imports: HashMap<String, ModIdx>,
    pub type_aliases: Vec<(usize, Ty)>,
    pub const_u32: HashMap<String, u32>,
}

pub struct ConstDeclInfo<'a> {
    pub module: ModIdx,
    pub name: String,
    pub ty: Option<Ty>,
    pub value: Option<&'a ast::Expr>,
}

pub struct Items<'a> {
    pub modules: Vec<ModuleScope>,
    pub mod_by_path: HashMap<String, ModIdx>,
    pub fns: Vec<FnDecl<'a>>,
    pub adts: Vec<AdtDecl>,
    pub foreigns: Vec<ForeignDeclInfo>,
    pub traits: Vec<TraitDecl<'a>>,
    pub trait_impls: Vec<TraitImplDecl>,
    pub inherent_impls: Vec<InherentImplDecl>,
    pub adt_kinds: Vec<AdtKind>,
    pub consts: Vec<ConstDeclInfo<'a>>,
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
    pub fn trait_decl(&self, t: TraitRef) -> &TraitDecl<'a> {
        &self.traits[t.0 as usize]
    }
    pub fn const_decl(&self, c: ConstRef) -> &ConstDeclInfo<'a> {
        &self.consts[c.0 as usize]
    }

    /// Resolves a bare or module-qualified item path from module `from`.
    pub fn eval_const_u32(&self, expr: &ast::Expr, from: ModIdx) -> Option<u32> {
        eval_const_u32(expr, from, &self.modules, &self.mod_by_path)
    }

    pub fn lookup_item(&self, from: ModIdx, path: &[String]) -> Option<ItemRef> {
        lookup_item_in(&self.modules, &self.mod_by_path, from, path)
    }

    /// Resolves a path to an ADT, following type aliases whose target is an ADT.
    pub fn resolve_adt_of(&self, from: ModIdx, path: &[String], types: &Types) -> Option<AdtRef> {
        match self.lookup_item(from, path) {
            Some(ItemRef::Adt(adt)) => Some(adt),
            Some(ItemRef::TypeAlias(ta_mod, ta_idx)) => {
                let (_, target) = self.modules[ta_mod.0 as usize].type_aliases[ta_idx as usize];
                if let TyData::Adt { adt, .. } = types.data(target) {
                    Some(*adt)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Returns all trait bounds entailed by `bound` (starting with `bound` itself,
    /// followed by transitive supertraits with substituted type arguments and
    /// associated type bindings). Guards against cyclic supertrait hierarchies.
    pub fn supertrait_closure(
        &self,
        bound: &TraitBound,
        self_ty: Option<Ty>,
        types: &mut Types,
    ) -> Vec<TraitBound> {
        let mut result = Vec::new();
        let mut visited: Vec<TraitRef> = Vec::new();
        let mut queue = std::collections::VecDeque::new();
        queue.push_back(bound.clone());
        while let Some(cur) = queue.pop_front() {
            if visited.contains(&cur.trait_ref) {
                continue;
            }
            visited.push(cur.trait_ref);
            let tr = self.trait_decl(cur.trait_ref);
            let supertraits = tr.supertraits.clone();
            for st in supertraits {
                let inst_args: Vec<Ty> = st
                    .trait_args
                    .iter()
                    .map(|t| match self_ty {
                        Some(st_ty) => types.subst_with_self(*t, &cur.trait_args, st_ty),
                        None => types.subst(*t, &cur.trait_args),
                    })
                    .collect();
                let inst_assoc: Vec<(String, Ty)> = st
                    .assoc_bindings
                    .iter()
                    .map(|(n, t)| {
                        let sub = match self_ty {
                            Some(st_ty) => types.subst_with_self(*t, &cur.trait_args, st_ty),
                            None => types.subst(*t, &cur.trait_args),
                        };
                        (n.clone(), sub)
                    })
                    .collect();
                queue.push_back(TraitBound {
                    trait_ref: st.trait_ref,
                    trait_args: inst_args,
                    assoc_bindings: inst_assoc,
                });
            }
            result.push(cur);
        }
        result
    }

    /// Whether `ty` is `Copy` under the generic environment `env_generics`.
    pub fn is_copy_in_env(&self, ty: Ty, env_generics: &[GenericInfo], types: &mut Types) -> bool {
        self.is_copy_depth(ty, env_generics, types, 0)
    }

    fn is_copy_depth(
        &self,
        ty: Ty,
        env_generics: &[GenericInfo],
        types: &mut Types,
        depth: u32,
    ) -> bool {
        if depth > 64 {
            return false;
        }
        let ty = self.normalize_ty_in_env(ty, env_generics, types);
        match types.data(types.shallow(ty)).clone() {
            TyData::Unit
            | TyData::Type
            | TyData::ConstParam(_)
            | TyData::ConstU32(_)
            | TyData::Bool
            | TyData::Never
            | TyData::Int(_)
            | TyData::Float(_)
            | TyData::Error => true,
            TyData::Tuple(items) | TyData::Union(items) => items
                .iter()
                .all(|t| self.is_copy_depth(*t, env_generics, types, depth + 1)),
            TyData::Array { element, .. } | TyData::ArrayConst { element, .. } => {
                self.is_copy_depth(element, env_generics, types, depth + 1)
            }
            TyData::Slice(_) | TyData::SelfParam => false,
            TyData::Ref { mutable, .. } => !mutable,
            TyData::Desc(_) | TyData::FnPtr { .. } => true,
            TyData::Closure { .. }
            | TyData::ImplCallable { .. }
            | TyData::Dyn { .. }
            | TyData::OwnedDyn { .. } => false,
            TyData::Managed(_) => false,
            TyData::Opaque(OpaqueKind::ForeignHandle | OpaqueKind::ForeignCallback { .. }) => true,
            TyData::Opaque(
                OpaqueKind::ByteBuffer
                | OpaqueKind::OwnedStore(_)
                | OpaqueKind::Mutex(_)
                | OpaqueKind::MutexGuard(_)
                | OpaqueKind::Task { .. }
                | OpaqueKind::Registration(_)
                | OpaqueKind::AsyncRegistration(_)
                | OpaqueKind::SyncCell(_),
            ) => false,
            TyData::ErrorUnion { ok, err } => {
                self.is_copy_depth(ok, env_generics, types, depth + 1)
                    && self.is_copy_depth(err, env_generics, types, depth + 1)
            }
            TyData::Param(i) => env_generics.get(i as usize).is_some_and(|g| g.copy),
            TyData::Adt { adt, args, .. } => {
                let decl = self.adt(adt);
                match decl.kind {
                    AdtKind::Record | AdtKind::Enum => {
                        let fields: Vec<Ty> = decl
                            .variants
                            .iter()
                            .flat_map(|v| v.fields.iter().map(|f| f.ty))
                            .collect();
                        fields.into_iter().all(|f| {
                            let t = types.subst(f, &args);
                            self.is_copy_depth(t, env_generics, types, depth + 1)
                        })
                    }
                    AdtKind::Package | AdtKind::Resource => false,
                }
            }
            TyData::Assoc { .. } | TyData::Var(_) => false,
        }
    }

    /// Matches a (possibly generic) trait impl's target pattern against `self_ty`
    /// and verifies all impl generic bounds and `where` predicates.
    /// Returns the substituted impl generic arguments if matched.
    pub fn match_impl(
        &self,
        imp: &TraitImplDecl,
        self_ty: Ty,
        types: &mut Types,
    ) -> Option<Vec<Ty>> {
        self.match_impl_depth(imp, self_ty, &[], types, 0)
    }

    /// Matches a trait impl against `self_ty` under the generic assumptions `env_generics`.
    pub fn match_impl_in_env(
        &self,
        imp: &TraitImplDecl,
        self_ty: Ty,
        env_generics: &[GenericInfo],
        types: &mut Types,
    ) -> Option<Vec<Ty>> {
        self.match_impl_depth(imp, self_ty, env_generics, types, 0)
    }

    fn match_impl_depth(
        &self,
        imp: &TraitImplDecl,
        self_ty: Ty,
        env_generics: &[GenericInfo],
        types: &mut Types,
        depth: u32,
    ) -> Option<Vec<Ty>> {
        if depth > 16 {
            return None;
        }
        let mut bindings: Vec<Option<Ty>> = vec![None; imp.generics.len()];
        if !match_template(imp.target, self_ty, types, &mut bindings) {
            return None;
        }
        let impl_args: Vec<Ty> = bindings.into_iter().collect::<Option<Vec<Ty>>>()?;
        // Verify all generic parameter bounds on the impl block.
        for (g, &arg) in imp.generics.iter().zip(impl_args.iter()) {
            let arg_shallow = types.shallow(arg);
            if matches!(types.data(arg_shallow), TyData::Error) {
                continue;
            }
            if g.copy && !self.is_copy_in_env(arg_shallow, env_generics, types) {
                return None;
            }
            if g.store_element
                && matches!(
                    types.data(arg_shallow),
                    TyData::Ref { .. } | TyData::Desc(_)
                )
            {
                return None;
            }
            for bound in &g.traits {
                let inst_bound = TraitBound {
                    trait_ref: bound.trait_ref,
                    trait_args: bound
                        .trait_args
                        .iter()
                        .map(|t| types.subst(*t, &impl_args))
                        .collect(),
                    assoc_bindings: bound
                        .assoc_bindings
                        .iter()
                        .map(|(n, t)| (n.clone(), types.subst(*t, &impl_args)))
                        .collect(),
                };
                if !self.satisfies_trait_bound_depth(
                    arg_shallow,
                    &inst_bound,
                    env_generics,
                    types,
                    depth + 1,
                ) {
                    return None;
                }
            }
        }
        // Verify additional `where` predicates on the impl block.
        for (subj, bound) in &imp.where_bounds {
            let inst_subj = types.subst(*subj, &impl_args);
            let inst_bound = TraitBound {
                trait_ref: bound.trait_ref,
                trait_args: bound
                    .trait_args
                    .iter()
                    .map(|t| types.subst(*t, &impl_args))
                    .collect(),
                assoc_bindings: bound
                    .assoc_bindings
                    .iter()
                    .map(|(n, t)| (n.clone(), types.subst(*t, &impl_args)))
                    .collect(),
            };
            if !self.satisfies_trait_bound_depth(
                inst_subj,
                &inst_bound,
                env_generics,
                types,
                depth + 1,
            ) {
                return None;
            }
        }
        Some(impl_args)
    }

    /// Checks whether `self_ty` satisfies `bound` (including supertrait entailment,
    /// associated type equality bindings, and blanket impl bounds) under `env_generics`.
    pub fn satisfies_trait_bound(
        &self,
        self_ty: Ty,
        bound: &TraitBound,
        env_generics: &[GenericInfo],
        types: &mut Types,
    ) -> bool {
        self.satisfies_trait_bound_depth(self_ty, bound, env_generics, types, 0)
    }

    fn satisfies_trait_bound_depth(
        &self,
        self_ty: Ty,
        bound: &TraitBound,
        env_generics: &[GenericInfo],
        types: &mut Types,
        depth: u32,
    ) -> bool {
        if depth > 16 {
            return false;
        }
        let self_ty = self.normalize_ty_in_env(self_ty, env_generics, types);
        let self_ty = types.shallow(self_ty);
        if matches!(types.data(self_ty), TyData::Error) {
            return true;
        }
        let req_args: Vec<Ty> = bound
            .trait_args
            .iter()
            .map(|t| self.normalize_ty_in_env(*t, env_generics, types))
            .collect();
        let req_assoc: Vec<(String, Ty)> = bound
            .assoc_bindings
            .iter()
            .map(|(n, t)| (n.clone(), self.normalize_ty_in_env(*t, env_generics, types)))
            .collect();

        // 1. Check generic parameter bounds and their transitive supertrait closure.
        if let TyData::Param(idx) = *types.data(self_ty)
            && let Some(g) = env_generics.get(idx as usize)
        {
            for g_bound in &g.traits {
                for entailed in self.supertrait_closure(g_bound, Some(self_ty), types) {
                    if entailed.trait_ref != bound.trait_ref {
                        continue;
                    }
                    let ent_args: Vec<Ty> = entailed
                        .trait_args
                        .iter()
                        .map(|t| self.normalize_ty_in_env(*t, env_generics, types))
                        .collect();
                    if ent_args != req_args {
                        continue;
                    }
                    let assoc_ok = req_assoc.iter().all(|(aname, req_ty)| {
                        if let Some((_, rhs)) =
                            entailed.assoc_bindings.iter().find(|(n, _)| n == aname)
                        {
                            let norm_rhs = self.normalize_ty_in_env(*rhs, env_generics, types);
                            norm_rhs == *req_ty
                        } else {
                            let proj = types.intern(TyData::Assoc {
                                base: self_ty,
                                trait_ref: Some(bound.trait_ref),
                                name: aname.clone(),
                            });
                            let norm_proj = self.normalize_ty_in_env(proj, env_generics, types);
                            norm_proj == *req_ty
                        }
                    });
                    if assoc_ok {
                        return true;
                    }
                }
            }
        }

        // 2. Check declared trait impls (concrete and blanket impls).
        for imp in &self.trait_impls {
            if imp.trait_ref != bound.trait_ref {
                continue;
            }
            let Some(impl_args) =
                self.match_impl_depth(imp, self_ty, env_generics, types, depth + 1)
            else {
                continue;
            };
            let imp_targs: Vec<Ty> = imp
                .trait_args
                .iter()
                .map(|t| {
                    let s = types.subst(*t, &impl_args);
                    self.normalize_ty_in_env(s, env_generics, types)
                })
                .collect();
            if imp_targs != req_args {
                continue;
            }
            let assoc_ok = req_assoc.iter().all(|(aname, req_ty)| {
                if let Some((_, rhs)) = imp.assoc_types.iter().find(|(n, _)| n == aname) {
                    let s = types.subst(*rhs, &impl_args);
                    let norm = self.normalize_ty_in_env(s, env_generics, types);
                    norm == *req_ty
                } else {
                    let proj = types.intern(TyData::Assoc {
                        base: self_ty,
                        trait_ref: None,
                        name: aname.clone(),
                    });
                    let norm = self.normalize_ty_in_env(proj, env_generics, types);
                    norm == *req_ty
                }
            });
            if assoc_ok {
                return true;
            }
        }

        false
    }

    /// Validates that `dyn Trait<trait_args, assoc_bindings>` is object-safe:
    /// - Trait generic argument arity matches the trait declaration.
    /// - Every associated type binding names a declared associated type on the trait.
    /// - Every method has a `self` receiver (`self`, `&self`, or `&mut self`), has no
    ///   method-level generic type parameters, and does not mention `Self` in any
    ///   non-receiver parameter or its return type after substituting `assoc_bindings`.
    pub fn check_dyn_object_safe(
        &self,
        tr: TraitRef,
        trait_args: &[Ty],
        assoc_bindings: &[(String, Ty)],
        types: &mut Types,
    ) -> Result<(), String> {
        let decl = self.trait_decl(tr);
        if trait_args.len() != decl.generics.len() {
            return Err(format!(
                "trait `{}` expects {} type argument(s), found {}",
                decl.name,
                decl.generics.len(),
                trait_args.len()
            ));
        }
        for (name, _) in assoc_bindings {
            if !decl.assoc_types.iter().any(|a| a == name) {
                return Err(format!(
                    "trait `{}` has no associated type `{name}`",
                    decl.name
                ));
            }
        }
        let self_param = types.intern(TyData::SelfParam);
        let decl_name = decl.name.clone();
        let methods = decl.methods.clone();
        for m in methods {
            if !m.generics.is_empty() {
                return Err(format!(
                    "trait `{decl_name}` is not object-safe: method `{}` has generic type parameters",
                    m.name
                ));
            }
            let Some(&first) = m.params.first() else {
                return Err(format!(
                    "trait `{decl_name}` is not object-safe: method `{}` has no `self` receiver",
                    m.name
                ));
            };
            let valid_receiver = match *types.data(types.shallow(first)) {
                TyData::SelfParam => true,
                TyData::Ref { pointee, .. } => {
                    matches!(*types.data(types.shallow(pointee)), TyData::SelfParam)
                }
                TyData::Unit
                | TyData::Type
                | TyData::ConstU32(_)
                | TyData::Bool
                | TyData::Never
                | TyData::Int(_)
                | TyData::Float(_)
                | TyData::Tuple(_)
                | TyData::Array { .. }
                | TyData::ArrayConst { .. }
                | TyData::Slice(_)
                | TyData::Adt { .. }
                | TyData::Managed(_)
                | TyData::Desc(_)
                | TyData::Opaque(_)
                | TyData::ErrorUnion { .. }
                | TyData::Union(_)
                | TyData::FnPtr { .. }
                | TyData::Closure { .. }
                | TyData::ImplCallable { .. }
                | TyData::Dyn { .. }
                | TyData::OwnedDyn { .. }
                | TyData::Param(_)
                | TyData::ConstParam(_)
                | TyData::Assoc { .. }
                | TyData::Var(_)
                | TyData::Error => false,
            };
            if !valid_receiver {
                return Err(format!(
                    "trait `{decl_name}` is not object-safe: method `{}` has no `self` receiver",
                    m.name
                ));
            }
            for &p in m.params.iter().skip(1) {
                let t = types.subst(p, trait_args);
                let t = types.subst_self_assoc(t, self_param, tr, assoc_bindings);
                if types.contains_self_param(t) {
                    return Err(format!(
                        "trait `{decl_name}` is not object-safe: method `{}` references `Self` in a parameter type",
                        m.name
                    ));
                }
            }
            let ret = types.subst(m.result, trait_args);
            let ret = types.subst_self_assoc(ret, self_param, tr, assoc_bindings);
            if types.contains_self_param(ret) {
                return Err(format!(
                    "trait `{decl_name}` is not object-safe: method `{}` references `Self` in its return type",
                    m.name
                ));
            }
        }
        Ok(())
    }

    /// Matches a (possibly generic) inherent impl's target pattern against `self_ty`.
    /// Returns the substituted impl generic arguments if matched.
    pub fn match_inherent_impl(
        &self,
        imp: &InherentImplDecl,
        self_ty: Ty,
        types: &Types,
    ) -> Option<Vec<Ty>> {
        let mut bindings: Vec<Option<Ty>> = vec![None; imp.generics.len()];
        if !match_template(imp.target, self_ty, types, &mut bindings) {
            return None;
        }
        bindings.into_iter().collect()
    }

    /// Normalizes associated type projections (`Drain<Item>.Item -> Item`, etc.)
    /// wherever the base type matches a declared trait impl.
    pub fn normalize_ty(&self, ty: Ty, types: &mut Types) -> Ty {
        self.normalize_ty_depth(ty, &[], types, 0)
    }

    /// Normalizes associated type projections under generic environment `env_generics`
    /// (so `T.Item` normalizes to `U` when `T: Container<Item = U>` is in scope).
    pub fn normalize_ty_in_env(
        &self,
        ty: Ty,
        env_generics: &[GenericInfo],
        types: &mut Types,
    ) -> Ty {
        self.normalize_ty_depth(ty, env_generics, types, 0)
    }

    fn normalize_ty_depth(
        &self,
        ty: Ty,
        env_generics: &[GenericInfo],
        types: &mut Types,
        depth: u32,
    ) -> Ty {
        if depth > 32 {
            return ty;
        }
        let ty = types.shallow(ty);
        match types.data(ty).clone() {
            TyData::Assoc {
                base,
                trait_ref,
                name,
            } => {
                let base = self.normalize_ty_depth(base, env_generics, types, depth + 1);
                // 1. Check generic parameter associated type equality bindings in scope.
                if let TyData::Param(idx) = *types.data(types.shallow(base))
                    && let Some(g) = env_generics.get(idx as usize)
                {
                    for g_bound in &g.traits {
                        for entailed in self.supertrait_closure(g_bound, Some(base), types) {
                            if trait_ref.is_some_and(|tr| tr != entailed.trait_ref) {
                                continue;
                            }
                            if let Some((_, rhs)) =
                                entailed.assoc_bindings.iter().find(|(n, _)| *n == name)
                            {
                                return self.normalize_ty_depth(
                                    *rhs,
                                    env_generics,
                                    types,
                                    depth + 1,
                                );
                            }
                        }
                    }
                }
                // 2. Check declared trait impls (respecting trait_ref when qualified).
                for imp in &self.trait_impls {
                    if trait_ref.is_some_and(|tr| tr != imp.trait_ref) {
                        continue;
                    }
                    if let Some((_, rhs)) = imp.assoc_types.iter().find(|(n, _)| *n == name)
                        && let Some(impl_args) =
                            self.match_impl_depth(imp, base, env_generics, types, 0)
                    {
                        let subst = types.subst(*rhs, &impl_args);
                        return self.normalize_ty_depth(subst, env_generics, types, depth + 1);
                    }
                }
                types.intern(TyData::Assoc {
                    base,
                    trait_ref,
                    name,
                })
            }
            TyData::Adt { adt, args, regions } => {
                let args = args
                    .into_iter()
                    .map(|a| self.normalize_ty_depth(a, env_generics, types, depth + 1))
                    .collect();
                types.intern(TyData::Adt { adt, args, regions })
            }
            TyData::Tuple(items) => {
                let items = items
                    .into_iter()
                    .map(|a| self.normalize_ty_depth(a, env_generics, types, depth + 1))
                    .collect();
                types.intern(TyData::Tuple(items))
            }
            TyData::Array { element, length } => {
                let element = self.normalize_ty_depth(element, env_generics, types, depth + 1);
                types.intern(TyData::Array { element, length })
            }
            TyData::Slice(element) => {
                let element = self.normalize_ty_depth(element, env_generics, types, depth + 1);
                types.intern(TyData::Slice(element))
            }
            TyData::Ref {
                region,
                mutable,
                pointee,
            } => {
                let pointee = self.normalize_ty_depth(pointee, env_generics, types, depth + 1);
                types.intern(TyData::Ref {
                    region,
                    mutable,
                    pointee,
                })
            }
            TyData::Managed(inner) => {
                let inner = self.normalize_ty_depth(inner, env_generics, types, depth + 1);
                types.intern(TyData::Managed(inner))
            }
            TyData::Desc(inner) => {
                let inner = self.normalize_ty_depth(inner, env_generics, types, depth + 1);
                types.intern(TyData::Desc(inner))
            }
            TyData::Opaque(OpaqueKind::OwnedStore(elem)) => {
                let elem = self.normalize_ty_depth(elem, env_generics, types, depth + 1);
                types.intern(TyData::Opaque(OpaqueKind::OwnedStore(elem)))
            }
            TyData::Opaque(OpaqueKind::Mutex(elem)) => {
                let elem = self.normalize_ty_depth(elem, env_generics, types, depth + 1);
                types.intern(TyData::Opaque(OpaqueKind::Mutex(elem)))
            }
            TyData::Opaque(OpaqueKind::MutexGuard(elem)) => {
                let elem = self.normalize_ty_depth(elem, env_generics, types, depth + 1);
                types.intern(TyData::Opaque(OpaqueKind::MutexGuard(elem)))
            }
            TyData::Opaque(OpaqueKind::Task { result, context }) => {
                let result = self.normalize_ty_depth(result, env_generics, types, depth + 1);
                let context =
                    context.map(|c| self.normalize_ty_depth(c, env_generics, types, depth + 1));
                types.intern(TyData::Opaque(OpaqueKind::Task { result, context }))
            }
            TyData::Opaque(OpaqueKind::Registration(elem)) => {
                let elem = self.normalize_ty_depth(elem, env_generics, types, depth + 1);
                types.intern(TyData::Opaque(OpaqueKind::Registration(elem)))
            }
            TyData::Opaque(OpaqueKind::AsyncRegistration(elem)) => {
                let elem = self.normalize_ty_depth(elem, env_generics, types, depth + 1);
                types.intern(TyData::Opaque(OpaqueKind::AsyncRegistration(elem)))
            }
            TyData::Opaque(OpaqueKind::ForeignCallback { params, result }) => {
                let params = params
                    .into_iter()
                    .map(|a| self.normalize_ty_depth(a, env_generics, types, depth + 1))
                    .collect();
                let result = self.normalize_ty_depth(result, env_generics, types, depth + 1);
                types.intern(TyData::Opaque(OpaqueKind::ForeignCallback {
                    params,
                    result,
                }))
            }
            TyData::Opaque(
                OpaqueKind::ByteBuffer | OpaqueKind::ForeignHandle | OpaqueKind::SyncCell(_),
            ) => ty,
            TyData::Union(items) => {
                let items = items
                    .into_iter()
                    .map(|a| self.normalize_ty_depth(a, env_generics, types, depth + 1))
                    .collect();
                types.canonicalize_union_raw(items)
            }
            TyData::ErrorUnion { ok, err } => {
                let ok = self.normalize_ty_depth(ok, env_generics, types, depth + 1);
                let err = self.normalize_ty_depth(err, env_generics, types, depth + 1);
                types.intern(TyData::ErrorUnion { ok, err })
            }
            TyData::FnPtr { params, result } => {
                let params = params
                    .into_iter()
                    .map(|a| self.normalize_ty_depth(a, env_generics, types, depth + 1))
                    .collect();
                let result = self.normalize_ty_depth(result, env_generics, types, depth + 1);
                types.intern(TyData::FnPtr { params, result })
            }
            TyData::Closure {
                owner,
                id,
                owner_args,
                kind,
                params,
                result,
            } => {
                let owner_args = owner_args
                    .into_iter()
                    .map(|a| self.normalize_ty_depth(a, env_generics, types, depth + 1))
                    .collect();
                let params = params
                    .into_iter()
                    .map(|a| self.normalize_ty_depth(a, env_generics, types, depth + 1))
                    .collect();
                let result = self.normalize_ty_depth(result, env_generics, types, depth + 1);
                types.intern(TyData::Closure {
                    owner,
                    id,
                    owner_args,
                    kind,
                    params,
                    result,
                })
            }
            TyData::ImplCallable {
                owner,
                owner_args,
                kind,
                params,
                result,
            } => {
                let owner_args = owner_args
                    .into_iter()
                    .map(|a| self.normalize_ty_depth(a, env_generics, types, depth + 1))
                    .collect();
                let params = params
                    .into_iter()
                    .map(|a| self.normalize_ty_depth(a, env_generics, types, depth + 1))
                    .collect();
                let result = self.normalize_ty_depth(result, env_generics, types, depth + 1);
                types.intern(TyData::ImplCallable {
                    owner,
                    owner_args,
                    kind,
                    params,
                    result,
                })
            }
            TyData::ArrayConst { element, length } => {
                let element = self.normalize_ty_depth(element, env_generics, types, depth + 1);
                let length = self.normalize_ty_depth(length, env_generics, types, depth + 1);
                if let TyData::ConstU32(n) = *types.data(length) {
                    types.intern(TyData::Array { element, length: n })
                } else {
                    types.intern(TyData::ArrayConst { element, length })
                }
            }
            TyData::Unit
            | TyData::Type
            | TyData::ConstParam(_)
            | TyData::ConstU32(_)
            | TyData::Bool
            | TyData::Never
            | TyData::Int(_)
            | TyData::Float(_)
            | TyData::Dyn { .. }
            | TyData::OwnedDyn { .. }
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
        (
            TyData::Adt {
                adt: a1, args: xs, ..
            },
            TyData::Adt {
                adt: a2, args: ys, ..
            },
        ) => {
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
            if parts.len() == 3 && parts[0] == "std" && parts[1] == "task" {
                match parts[2].as_str() {
                    "Poll" => return Some(ItemRef::Adt(AdtRef(2))),
                    "TaskCreateError" => return Some(ItemRef::Adt(AdtRef(3))),
                    "TaskPollError" | "TaskPollState" => return Some(ItemRef::Adt(AdtRef(4))),
                    "TaskCreateOwnedFailure" => return Some(ItemRef::Adt(AdtRef(6))),
                    "TaskThreadScopeResult" => return Some(ItemRef::Adt(AdtRef(7))),
                    _ => {}
                }
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
        "Type" => TyData::Type,
        "c_handle" => TyData::Opaque(OpaqueKind::ForeignHandle),
        "AtomicI32" => TyData::Opaque(OpaqueKind::SyncCell(zil_ir::body::SyncCellKind::I32)),
        "AtomicI64" => TyData::Opaque(OpaqueKind::SyncCell(zil_ir::body::SyncCellKind::I64)),
        "AtomicUsize" => TyData::Opaque(OpaqueKind::SyncCell(zil_ir::body::SyncCellKind::Usize)),
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
        inherent_impls: Vec::new(),
        adt_kinds: Vec::new(),
        consts: Vec::new(),
    };
    for (i, inp) in inputs.iter().enumerate() {
        let idx = ModIdx(i as u32);
        items.modules.push(ModuleScope {
            path: inp.path.clone(),
            names: HashMap::new(),
            publics: HashSet::new(),
            imports: HashMap::new(),
            type_aliases: Vec::new(),
            const_u32: HashMap::new(),
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
        region_params: 0,
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
                    default: None,
                    public: true,
                }],
            },
        ],
        module: ModIdx(0),
        methods: Vec::new(),
        user_drop: false,
    });
    let dyn_create_error_adt = AdtRef(1);
    items.adt_kinds.push(AdtKind::Enum);
    items.adts.push(AdtDecl {
        name: "DynCreateError".to_string(),
        kind: AdtKind::Enum,
        generics: Vec::new(),
        region_params: 0,
        variants: vec![VariantDecl {
            name: "Allocation".to_string(),
            shape: VariantShape::Unit,
            fields: Vec::new(),
        }],
        module: ModIdx(0),
        methods: Vec::new(),
        user_drop: false,
    });
    let poll_adt = AdtRef(2);
    items.adt_kinds.push(AdtKind::Enum);
    items.adts.push(AdtDecl {
        name: "Poll".to_string(),
        kind: AdtKind::Enum,
        generics: vec!["T".to_string()],
        region_params: 0,
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
                    default: None,
                    public: true,
                }],
            },
        ],
        module: ModIdx(0),
        methods: Vec::new(),
        user_drop: false,
    });
    let task_create_error_adt = AdtRef(3);
    items.adt_kinds.push(AdtKind::Enum);
    items.adts.push(AdtDecl {
        name: "TaskCreateError".to_string(),
        kind: AdtKind::Enum,
        generics: Vec::new(),
        region_params: 0,
        variants: vec![VariantDecl {
            name: "Allocation".to_string(),
            shape: VariantShape::Unit,
            fields: Vec::new(),
        }],
        module: ModIdx(0),
        methods: Vec::new(),
        user_drop: false,
    });
    let task_poll_error_adt = AdtRef(4);
    items.adt_kinds.push(AdtKind::Enum);
    items.adts.push(AdtDecl {
        name: "TaskPollState".to_string(),
        kind: AdtKind::Enum,
        generics: Vec::new(),
        region_params: 0,
        variants: vec![
            VariantDecl {
                name: "Cancelled".to_string(),
                shape: VariantShape::Unit,
                fields: Vec::new(),
            },
            VariantDecl {
                name: "Complete".to_string(),
                shape: VariantShape::Unit,
                fields: Vec::new(),
            },
            VariantDecl {
                name: "Invalid".to_string(),
                shape: VariantShape::Unit,
                fields: Vec::new(),
            },
        ],
        module: ModIdx(0),
        methods: Vec::new(),
        user_drop: false,
    });
    let mutex_create_error_adt = AdtRef(5);
    items.adt_kinds.push(AdtKind::Enum);
    items.adts.push(AdtDecl {
        name: "MutexCreateError".to_string(),
        kind: AdtKind::Enum,
        generics: Vec::new(),
        region_params: 0,
        variants: vec![VariantDecl {
            name: "Allocation".to_string(),
            shape: VariantShape::Unit,
            fields: Vec::new(),
        }],
        module: ModIdx(0),
        methods: Vec::new(),
        user_drop: false,
    });
    let task_create_error_ty = types.intern(TyData::Adt {
        adt: task_create_error_adt,
        args: Vec::new(),
        regions: Vec::new(),
    });
    let task_create_owned_failure_adt = AdtRef(6);
    items.adt_kinds.push(AdtKind::Package);
    items.adts.push(AdtDecl {
        name: "TaskCreateOwnedFailure".to_string(),
        kind: AdtKind::Package,
        generics: vec!["Inputs".to_string()],
        region_params: 0,
        variants: vec![VariantDecl {
            name: "TaskCreateOwnedFailure".to_string(),
            shape: VariantShape::Record,
            fields: vec![
                FieldDecl {
                    name: "reason".to_string(),
                    ty: task_create_error_ty,
                    default: None,
                    public: true,
                },
                FieldDecl {
                    name: "inputs".to_string(),
                    ty: option_t,
                    default: None,
                    public: true,
                },
            ],
        }],
        module: ModIdx(0),
        methods: Vec::new(),
        user_drop: false,
    });
    let ts_r1 = types.intern(TyData::Param(1));
    let ts_r2 = types.intern(TyData::Param(2));
    let task_thread_scope_result_adt = AdtRef(7);
    items.adt_kinds.push(AdtKind::Enum);
    items.adts.push(AdtDecl {
        name: "TaskThreadScopeResult".to_string(),
        kind: AdtKind::Enum,
        generics: vec!["T".to_string(), "R1".to_string(), "R2".to_string()],
        region_params: 0,
        variants: vec![
            VariantDecl {
                name: "Failed".to_string(),
                shape: VariantShape::Tuple,
                fields: vec![FieldDecl {
                    name: "0".to_string(),
                    ty: option_t,
                    default: None,
                    public: true,
                }],
            },
            VariantDecl {
                name: "Completed".to_string(),
                shape: VariantShape::Tuple,
                fields: vec![
                    FieldDecl {
                        name: "0".to_string(),
                        ty: ts_r1,
                        default: None,
                        public: true,
                    },
                    FieldDecl {
                        name: "1".to_string(),
                        ty: ts_r2,
                        default: None,
                        public: true,
                    },
                ],
            },
        ],
        module: ModIdx(0),
        methods: Vec::new(),
        user_drop: false,
    });
    let memory_order_adt = AdtRef(8);
    items.adt_kinds.push(AdtKind::Enum);
    items.adts.push(AdtDecl {
        name: "MemoryOrder".to_string(),
        kind: AdtKind::Enum,
        generics: Vec::new(),
        region_params: 0,
        variants: vec![
            VariantDecl {
                name: "Relaxed".to_string(),
                shape: VariantShape::Unit,
                fields: Vec::new(),
            },
            VariantDecl {
                name: "Acquire".to_string(),
                shape: VariantShape::Unit,
                fields: Vec::new(),
            },
            VariantDecl {
                name: "Release".to_string(),
                shape: VariantShape::Unit,
                fields: Vec::new(),
            },
            VariantDecl {
                name: "AcqRel".to_string(),
                shape: VariantShape::Unit,
                fields: Vec::new(),
            },
            VariantDecl {
                name: "SeqCst".to_string(),
                shape: VariantShape::Unit,
                fields: Vec::new(),
            },
        ],
        module: ModIdx(0),
        methods: Vec::new(),
        user_drop: false,
    });

    // Pass 1: top-level names, ADT kinds and module imports.
    let mut adt_asts: Vec<(ModIdx, AdtAst<'a>)> = Vec::new();
    let mut fn_asts: Vec<(ModIdx, &'a ast::Function)> = Vec::new();
    let mut foreign_asts: Vec<(ModIdx, &str, &'a ast::Function, Span)> = Vec::new();
    let mut impl_asts: Vec<(ModIdx, &'a ast::ImplBlock, Span)> = Vec::new();
    let mut trait_asts: Vec<(ModIdx, &'a ast::TraitDecl, Span)> = Vec::new();
    let mut alias_asts: Vec<(ModIdx, u32, &'a ast::TypeAlias, Span)> = Vec::new();
    let mut const_asts: Vec<(ModIdx, ConstRef, &'a ast::ConstDecl, Span)> = Vec::new();

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
                ast::ItemKind::TypeAlias(ta) => {
                    let arity = ta
                        .generics
                        .iter()
                        .filter(|g| matches!(g.kind, ast::GenericParamKind::Type(_)))
                        .count();
                    let ta_idx = items.modules[m_i].type_aliases.len() as u32;
                    items.modules[m_i].type_aliases.push((arity, types.error()));
                    alias_asts.push((mid, ta_idx, ta, item.span));
                    (&ta.name, ItemRef::TypeAlias(mid, ta_idx))
                }
                ast::ItemKind::Const(c) => {
                    let cid = ConstRef(items.consts.len() as u32);
                    items.consts.push(ConstDeclInfo {
                        module: mid,
                        name: c.name.clone(),
                        ty: None,
                        value: c.value.as_ref(),
                    });
                    const_asts.push((mid, cid, c, item.span));
                    (&c.name, ItemRef::Const(cid))
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
            .entry("TaskPollState".to_string())
            .or_insert(ItemRef::Adt(task_poll_error_adt));
        items.modules[m_i]
            .names
            .entry("MutexCreateError".to_string())
            .or_insert(ItemRef::Adt(mutex_create_error_adt));
        items.modules[m_i]
            .names
            .entry("TaskCreateOwnedFailure".to_string())
            .or_insert(ItemRef::Adt(task_create_owned_failure_adt));
        items.modules[m_i]
            .names
            .entry("TaskThreadScopeResult".to_string())
            .or_insert(ItemRef::Adt(task_thread_scope_result_adt));
        items.modules[m_i]
            .names
            .entry("MemoryOrder".to_string())
            .or_insert(ItemRef::Adt(memory_order_adt));
    }

    // Pass 1a: evaluate integer const items for fixed array lengths `[T; N]`.
    let max_const_passes = const_asts.len().max(1);
    for _ in 0..max_const_passes {
        let mut progress = false;
        for &(mid, _, c, _) in &const_asts {
            if items.modules[mid.0 as usize]
                .const_u32
                .contains_key(&c.name)
            {
                continue;
            }
            if let Some(expr) = &c.value
                && let Some(val) = eval_const_u32(expr, mid, &items.modules, &items.mod_by_path)
            {
                items.modules[mid.0 as usize]
                    .const_u32
                    .insert(c.name.clone(), val);
                progress = true;
            }
        }
        if !progress {
            break;
        }
    }

    // Pass 1b: resolve type aliases (fixed-point loop for chained aliases).
    let max_passes = alias_asts.len().max(1);
    let mut alias_resolved = vec![false; alias_asts.len()];
    for pass in 0..max_passes {
        let mut progress = false;
        for (i, &(mid, ta_idx, ta, span)) in alias_asts.iter().enumerate() {
            if alias_resolved[i] {
                continue;
            }
            let Some(val_ast) = &ta.value else {
                diags.push(diag::error(
                    diag::NAME_UNRESOLVED,
                    span,
                    format!("type alias `{}` requires a target type", ta.name),
                ));
                alias_resolved[i] = true;
                continue;
            };
            let gen_names: Vec<String> = ta
                .generics
                .iter()
                .filter(|g| matches!(g.kind, ast::GenericParamKind::Type(_)))
                .map(|g| g.name.clone())
                .collect();
            let lt_names: Vec<String> = ta
                .generics
                .iter()
                .filter(|g| g.kind == ast::GenericParamKind::Lifetime)
                .map(|g| g.name.clone())
                .collect();
            match resolve_type_with_lifetimes(
                val_ast,
                mid,
                &gen_names,
                &lt_names,
                &items.modules,
                &items.mod_by_path,
                &items.adt_kinds,
                None,
                None,
                types,
            ) {
                Ok(ty) => {
                    if *types.data(ty) != TyData::Error {
                        items.modules[mid.0 as usize].type_aliases[ta_idx as usize].1 = ty;
                        alias_resolved[i] = true;
                        progress = true;
                    } else if pass + 1 == max_passes {
                        diags.push(diag::error(
                            diag::NAME_UNRESOLVED,
                            span,
                            format!("cyclic or unresolved type alias `{}`", ta.name),
                        ));
                        alias_resolved[i] = true;
                    }
                }
                Err(d) => {
                    if pass + 1 == max_passes {
                        diags.push(d);
                        alias_resolved[i] = true;
                    }
                }
            }
        }
        if !progress {
            break;
        }
    }

    // Pass 2: declarations. Collect traits first so function/ADT generic bounds can reference them.
    for (ti, (mid, tr, span)) in trait_asts.into_iter().enumerate() {
        let decl = collect_trait(
            TraitRef(ti as u32),
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
    check_supertrait_cycles(&items.traits, diags);
    let mut adt_method_asts: Vec<(ModIdx, AdtRef, &'a ast::Aggregate)> = Vec::new();
    for (mid, item) in adt_asts {
        let adt_ref = AdtRef(items.adts.len() as u32);
        if let AdtAst::Aggregate(agg, _) = item
            && !agg.methods.is_empty()
        {
            adt_method_asts.push((mid, adt_ref, agg));
        }
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
            &items.adts,
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
            &items.adts,
            types,
            diags,
        );
        items.foreigns.push(decl);
    }
    for (mid, cid, c, _) in const_asts {
        if let Some(ty_ast) = &c.ty {
            let resolved = resolve_type(ty_ast, mid, &[], &[], &items, types);
            match resolved {
                Ok(ty) => items.consts[cid.0 as usize].ty = Some(ty),
                Err(d) => diags.push(d),
            }
        }
    }
    for (mid, adt_ref, agg) in adt_method_asts {
        collect_adt_methods(mid, adt_ref, agg, &mut items, types, diags);
    }
    for (mid, imp, span) in impl_asts {
        collect_impl(mid, imp, span, &mut items, types, diags);
    }
    check_trait_impl_supertraits(&items, types, diags);
    items
}

/// Detects and rejects cyclic supertrait hierarchies (e.g. `trait A: B; trait B: A`).
fn check_supertrait_cycles(traits: &[TraitDecl<'_>], diags: &mut Vec<Diagnostic>) {
    // 0 = unvisited, 1 = visiting (on active stack), 2 = visited.
    let mut state = vec![0u8; traits.len()];
    let mut reported = vec![false; traits.len()];
    fn dfs(
        cur: usize,
        traits: &[TraitDecl<'_>],
        state: &mut [u8],
        reported: &mut [bool],
        diags: &mut Vec<Diagnostic>,
    ) {
        state[cur] = 1;
        for st in &traits[cur].supertraits {
            let next = st.trait_ref.0 as usize;
            if next >= traits.len() {
                continue;
            }
            if state[next] == 1 {
                if !reported[cur] {
                    reported[cur] = true;
                    diags.push(diag::error(
                        diag::GENERIC_BOUND,
                        traits[cur].span,
                        format!(
                            "cyclic supertrait hierarchy involving `{}` and `{}`",
                            traits[cur].name, traits[next].name
                        ),
                    ));
                }
            } else if state[next] == 0 {
                dfs(next, traits, state, reported, diags);
            }
        }
        state[cur] = 2;
    }
    for i in 0..traits.len() {
        if state[i] == 0 {
            dfs(i, traits, &mut state, &mut reported, diags);
        }
    }
}

/// Verifies that every `impl Sub for T` satisfies all supertrait bounds of `Sub`.
fn check_trait_impl_supertraits(items: &Items<'_>, types: &mut Types, diags: &mut Vec<Diagnostic>) {
    let impls = items.trait_impls.clone();
    for imp in &impls {
        let tr = items.trait_decl(imp.trait_ref);
        if tr.supertraits.is_empty() {
            continue;
        }
        let sub_name = tr.name.clone();
        let supertraits = tr.supertraits.clone();
        for st in &supertraits {
            let inst_bound = TraitBound {
                trait_ref: st.trait_ref,
                trait_args: st
                    .trait_args
                    .iter()
                    .map(|t| types.subst_with_self(*t, &imp.trait_args, imp.target))
                    .collect(),
                assoc_bindings: st
                    .assoc_bindings
                    .iter()
                    .map(|(n, t)| {
                        (
                            n.clone(),
                            types.subst_with_self(*t, &imp.trait_args, imp.target),
                        )
                    })
                    .collect(),
            };
            if !items.satisfies_trait_bound(imp.target, &inst_bound, &imp.generics, types) {
                let super_name = &items.trait_decl(st.trait_ref).name;
                diags.push(diag::error(
                    diag::GENERIC_BOUND,
                    imp.span,
                    format!(
                        "supertrait `{super_name}` is not implemented for the target of `impl {sub_name}`"
                    ),
                ));
            }
        }
    }
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
fn resolve_trait_bound_args(
    args: &[ast::GenericArg],
    span: Span,
    mid: ModIdx,
    scope: &[String],
    lifetimes: &[String],
    modules: &[ModuleScope],
    mod_by_path: &HashMap<String, ModIdx>,
    adt_kinds: &[AdtKind],
    self_ty: Option<Ty>,
    types: &mut Types,
    diags: &mut Vec<Diagnostic>,
) -> (Vec<Ty>, Vec<(String, Ty)>) {
    let mut targs = Vec::new();
    let mut assoc_bindings = Vec::new();
    for a in args {
        match a {
            ast::GenericArg::Type(t) => {
                targs.push(
                    resolve_type_with_lifetimes(
                        t,
                        mid,
                        scope,
                        lifetimes,
                        modules,
                        mod_by_path,
                        adt_kinds,
                        self_ty,
                        None,
                        types,
                    )
                    .unwrap_or_else(|d| {
                        diags.push(d);
                        types.error()
                    }),
                );
            }
            ast::GenericArg::AssociatedBinding(binding) => match &binding.kind {
                ast::DynBindingKind::Type { member, value } if member.len() == 1 => {
                    let val_ty = resolve_type_with_lifetimes(
                        value,
                        mid,
                        scope,
                        lifetimes,
                        modules,
                        mod_by_path,
                        adt_kinds,
                        self_ty,
                        None,
                        types,
                    )
                    .unwrap_or_else(|d| {
                        diags.push(d);
                        types.error()
                    });
                    assoc_bindings.push((member[0].clone(), val_ty));
                }
                ast::DynBindingKind::Type { .. }
                | ast::DynBindingKind::Constant { .. }
                | ast::DynBindingKind::Method { .. } => {
                    diags.push(diag::unsupported(
                        "unsupported associated binding in trait bound",
                        binding.span,
                    ));
                }
            },
            ast::GenericArg::Lifetime(_) => {}
            ast::GenericArg::Const(_) => {
                diags.push(diag::unsupported("non-type trait arguments", span));
            }
        }
    }
    (targs, assoc_bindings)
}

#[allow(clippy::too_many_arguments)]
fn apply_single_bound(
    b: &ast::TypeRef,
    info: &mut GenericInfo,
    mid: ModIdx,
    scope: &[String],
    lifetimes: &[String],
    modules: &[ModuleScope],
    mod_by_path: &HashMap<String, ModIdx>,
    adt_kinds: &[AdtKind],
    types: &mut Types,
    diags: &mut Vec<Diagnostic>,
) {
    if let Some(c) =
        resolve_callable_bound(b, mid, scope, modules, mod_by_path, adt_kinds, types, diags)
    {
        info.callable = Some(c);
        return;
    }
    if let ast::TypeKind::Named { path, args } = &b.kind {
        if args.is_empty() && path == &["Copy"] {
            info.copy = true;
        } else if args.is_empty() && path == &["StoreElement"] {
            info.store_element = true;
        } else if args.is_empty() && path == &["SyncDrop"] {
            info.sync_drop = true;
        } else if args.is_empty()
            && matches!(
                path.as_slice(),
                [s] if matches!(
                    s.as_str(),
                    "Value" | "ClosedEnvironment" | "Transfer" | "Share" | "ThreadCallable"
                )
            )
        {
        } else if let Some(ItemRef::Trait(tr)) = lookup_item_in(modules, mod_by_path, mid, path) {
            let (trait_args, assoc_bindings) = resolve_trait_bound_args(
                args,
                b.span,
                mid,
                scope,
                lifetimes,
                modules,
                mod_by_path,
                adt_kinds,
                None,
                types,
                diags,
            );
            info.traits.push(TraitBound {
                trait_ref: tr,
                trait_args,
                assoc_bindings,
            });
        } else {
            diags.push(diag::unsupported("unknown trait bound", b.span));
        }
    } else {
        diags.push(diag::unsupported("non-named trait bound", b.span));
    }
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
    type_generics_in_scope(
        mid,
        generics,
        &[],
        &[],
        modules,
        mod_by_path,
        adt_kinds,
        types,
        diags,
    )
}

#[allow(clippy::too_many_arguments)]
fn type_generics_in_scope(
    mid: ModIdx,
    generics: &[ast::GenericParam],
    outer_scope: &[String],
    outer_lifetimes: &[String],
    modules: &[ModuleScope],
    mod_by_path: &HashMap<String, ModIdx>,
    adt_kinds: &[AdtKind],
    types: &mut Types,
    diags: &mut Vec<Diagnostic>,
) -> (Vec<GenericInfo>, Vec<String>) {
    let mut scope: Vec<String> = outer_scope.to_vec();
    scope.extend(
        generics
            .iter()
            .filter(|g| matches!(g.kind, ast::GenericParamKind::Type(_)))
            .map(|g| g.name.clone()),
    );
    let mut regions = Vec::new();
    for g in generics {
        if matches!(g.kind, ast::GenericParamKind::Lifetime) {
            regions.push(g.name.clone());
        }
    }
    let mut full_lifetimes: Vec<String> = outer_lifetimes.to_vec();
    full_lifetimes.extend(regions.iter().cloned());

    let mut out = Vec::new();
    for g in generics {
        match &g.kind {
            ast::GenericParamKind::Type(bounds) => {
                let mut info = GenericInfo {
                    name: g.name.clone(),
                    copy: false,
                    store_element: false,
                    sync_drop: false,
                    traits: Vec::new(),
                    callable: None,
                    const_ty: None,
                };
                for b in bounds {
                    apply_single_bound(
                        b,
                        &mut info,
                        mid,
                        &scope,
                        &full_lifetimes,
                        modules,
                        mod_by_path,
                        adt_kinds,
                        types,
                        diags,
                    );
                }
                out.push(info);
            }
            ast::GenericParamKind::Lifetime => {}
            ast::GenericParamKind::Const(ty_ref) => {
                let cty = match resolve_type_with_lifetimes(
                    ty_ref,
                    mid,
                    &[],
                    &[],
                    modules,
                    mod_by_path,
                    adt_kinds,
                    None,
                    None,
                    types,
                ) {
                    Ok(t) => t,
                    Err(d) => {
                        diags.push(d);
                        types.error()
                    }
                };
                if !matches!(types.data(cty), TyData::Int(_) | TyData::Error) {
                    diags.push(diag::unsupported(
                        "non-integer const generic parameter",
                        ty_ref.span,
                    ));
                }
                out.push(GenericInfo {
                    name: g.name.clone(),
                    copy: true,
                    store_element: false,
                    sync_drop: false,
                    traits: Vec::new(),
                    callable: None,
                    const_ty: Some(cty),
                });
            }
        }
    }
    (out, regions)
}

/// Applies `where`-clause predicates onto `generics` (for type-parameter subjects)
/// and returns any remaining non-parameter `(subject_ty, TraitBound)` predicates.
#[allow(clippy::too_many_arguments)]
fn apply_where_clause(
    where_clause: &[ast::BoundClause],
    generics: &mut [GenericInfo],
    scope: &[String],
    lifetimes: &[String],
    mid: ModIdx,
    modules: &[ModuleScope],
    mod_by_path: &HashMap<String, ModIdx>,
    adt_kinds: &[AdtKind],
    types: &mut Types,
    diags: &mut Vec<Diagnostic>,
) -> Vec<(Ty, TraitBound)> {
    let mut extra = Vec::new();
    for clause in where_clause {
        match &clause.subject {
            ast::BoundSubject::Lifetime(_) => {}
            ast::BoundSubject::Type(subj_ty) => {
                if let ast::TypeKind::Named { path, args } = &subj_ty.kind
                    && args.is_empty()
                    && let [name] = path.as_slice()
                    && let Some(idx) = scope.iter().rposition(|g| g == name)
                    && idx < generics.len()
                {
                    for b in &clause.bounds {
                        apply_single_bound(
                            b,
                            &mut generics[idx],
                            mid,
                            scope,
                            lifetimes,
                            modules,
                            mod_by_path,
                            adt_kinds,
                            types,
                            diags,
                        );
                    }
                } else {
                    let resolved_subj = resolve_type_with_lifetimes(
                        subj_ty,
                        mid,
                        scope,
                        lifetimes,
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
                    for b in &clause.bounds {
                        if let ast::TypeKind::Named { path, args } = &b.kind {
                            if args.is_empty()
                                && matches!(
                                    path.as_slice(),
                                    [s] if matches!(
                                        s.as_str(),
                                        "Copy"
                                            | "StoreElement"
                                            | "SyncDrop"
                                            | "Value"
                                            | "ClosedEnvironment"
                                            | "Transfer"
                                            | "Share"
                                            | "ThreadCallable"
                                    )
                                )
                            {
                            } else if let Some(ItemRef::Trait(tr)) =
                                lookup_item_in(modules, mod_by_path, mid, path)
                            {
                                let (trait_args, assoc_bindings) = resolve_trait_bound_args(
                                    args,
                                    b.span,
                                    mid,
                                    scope,
                                    lifetimes,
                                    modules,
                                    mod_by_path,
                                    adt_kinds,
                                    None,
                                    types,
                                    diags,
                                );
                                extra.push((
                                    resolved_subj,
                                    TraitBound {
                                        trait_ref: tr,
                                        trait_args,
                                        assoc_bindings,
                                    },
                                ));
                            } else {
                                diags.push(diag::unsupported(
                                    "unsupported where-clause bound",
                                    b.span,
                                ));
                            }
                        } else {
                            diags.push(diag::unsupported("unsupported where-clause bound", b.span));
                        }
                    }
                }
            }
        }
    }
    extra
}

#[allow(clippy::too_many_arguments)]
fn collect_trait<'a>(
    trait_ref: TraitRef,
    mid: ModIdx,
    tr: &'a ast::TraitDecl,
    span: Span,
    modules: &[ModuleScope],
    mod_by_path: &HashMap<String, ModIdx>,
    adt_kinds: &[AdtKind],
    types: &mut Types,
    diags: &mut Vec<Diagnostic>,
) -> TraitDecl<'a> {
    let (generics, tr_lifetimes) = type_generics(
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

    // Collect declared associated type names on this trait.
    let mut assoc_types = Vec::new();
    for m in &tr.members {
        if let ast::ItemKind::TypeAlias(ta) = &m.kind {
            assoc_types.push(ta.name.clone());
        }
    }

    // Collect supertrait bounds (`trait Sub: Super1 + Super2<T>`).
    let mut supertraits = Vec::new();
    for b in &tr.bounds {
        if let ast::TypeKind::Named { path, args } = &b.kind {
            if args.is_empty()
                && matches!(
                    path.as_slice(),
                    [s] if matches!(
                        s.as_str(),
                        "Copy" | "StoreElement" | "SyncDrop" | "Value" | "ClosedEnvironment"
                            | "Transfer" | "Share" | "ThreadCallable"
                    )
                )
            {
                continue;
            } else if let Some(ItemRef::Trait(super_ref)) =
                lookup_item_in(modules, mod_by_path, mid, path)
            {
                let (trait_args, assoc_bindings) = resolve_trait_bound_args(
                    args,
                    b.span,
                    mid,
                    &scope,
                    &tr_lifetimes,
                    modules,
                    mod_by_path,
                    adt_kinds,
                    Some(self_param),
                    types,
                    diags,
                );
                supertraits.push(TraitBound {
                    trait_ref: super_ref,
                    trait_args,
                    assoc_bindings,
                });
            } else {
                diags.push(diag::error(
                    diag::NAME_UNRESOLVED,
                    b.span,
                    format!("unknown supertrait `{}`", path.join(".")),
                ));
            }
        } else {
            diags.push(diag::unsupported("non-named supertrait bound", b.span));
        }
    }

    let mut methods = Vec::new();
    for m in &tr.members {
        match &m.kind {
            ast::ItemKind::TypeAlias(_) => continue,
            ast::ItemKind::Function(f) => {
                let (mut m_generics, m_lifetimes) = type_generics_in_scope(
                    mid,
                    &f.generics,
                    &scope,
                    &tr_lifetimes,
                    modules,
                    mod_by_path,
                    adt_kinds,
                    types,
                    diags,
                );
                let mut m_scope = scope.clone();
                m_scope.extend(m_generics.iter().map(|g| g.name.clone()));
                let mut full_lifetimes = tr_lifetimes.clone();
                full_lifetimes.extend(m_lifetimes.iter().cloned());
                if !f.where_clause.is_empty() {
                    let mut combined_generics = generics.clone();
                    combined_generics.extend(m_generics);
                    let _ = apply_where_clause(
                        &f.where_clause,
                        &mut combined_generics,
                        &m_scope,
                        &full_lifetimes,
                        mid,
                        modules,
                        mod_by_path,
                        adt_kinds,
                        types,
                        diags,
                    );
                    m_generics = combined_generics.split_off(generics.len());
                }
                let mut params = Vec::new();
                for p in &f.params {
                    let ty = resolve_type_with_lifetimes(
                        &p.ty,
                        mid,
                        &m_scope,
                        &full_lifetimes,
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
                    let ty = qualify_self_assoc(ty, self_param, trait_ref, &assoc_types, types);
                    params.push(ty);
                }
                let result = match &f.return_type {
                    Some(t) => {
                        let ty = resolve_type_with_lifetimes(
                            t,
                            mid,
                            &m_scope,
                            &full_lifetimes,
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
                        qualify_self_assoc(ty, self_param, trait_ref, &assoc_types, types)
                    }
                    None => types.unit(),
                };
                methods.push(TraitMethodDecl {
                    name: f.name.clone(),
                    generics: m_generics,
                    region_params: m_lifetimes,
                    params,
                    result,
                    default_ast: if matches!(f.body, ast::FunctionBody::Declaration) {
                        None
                    } else {
                        Some(f)
                    },
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
        module: mid,
        name: qualify(&modules[mid.0 as usize].path, &tr.name),
        generics,
        supertraits,
        assoc_types,
        methods,
        span,
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
        | TyData::Type
        | TyData::ConstParam(_)
        | TyData::ConstU32(_)
        | TyData::ArrayConst { .. }
        | TyData::Bool
        | TyData::Never
        | TyData::Int(_)
        | TyData::Float(_)
        | TyData::Adt { .. }
        | TyData::Tuple(_)
        | TyData::Union(_)
        | TyData::Array { .. }
        | TyData::Slice(_)
        | TyData::Ref { .. }
        | TyData::Opaque(_)
        | TyData::ErrorUnion { .. }
        | TyData::FnPtr { .. }
        | TyData::Closure { .. }
        | TyData::ImplCallable { .. }
        | TyData::Dyn { .. }
        | TyData::OwnedDyn { .. }
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
    if let TyData::Union(members) = types.data(ty).clone() {
        let mut changed = false;
        let mut next = Vec::with_capacity(members.len());
        for m in members {
            if let Some(pm) = promote_to_managed(m, types) {
                if pm != m {
                    changed = true;
                }
                next.push(pm);
            } else {
                next.push(m);
            }
        }
        if changed {
            return Some(types.canonicalize_union_raw(next));
        }
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
            (&a.name, kind, &a.generics, span)
        }
        AdtAst::Enum(e, span) => (&e.name, AdtKind::Enum, &e.generics, span),
    };
    let (generic_info, lifetimes) =
        type_generics(mid, generics, modules, mod_by_path, adt_kinds, types, diags);
    let scope: Vec<String> = generic_info.iter().map(|g| g.name.clone()).collect();
    let mut resolve = |t: &ast::TypeRef, diags: &mut Vec<Diagnostic>, types: &mut Types| {
        resolve_type_with_lifetimes(
            t,
            mid,
            &scope,
            &lifetimes,
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
                    default: f.default.clone(),
                    public: owner_kind == AdtKind::Enum || f.public,
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
                            let raw_ty = resolve(&f.ty, diags, types);
                            let ty = if f.manages {
                                if let Some(managed) = promote_to_managed(raw_ty, types) {
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
                                raw_ty
                            };
                            fs.push(FieldDecl {
                                name: i.to_string(),
                                ty,
                                default: None,
                                public: true,
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
        module: mid,
        name: qualify(&modules[mid.0 as usize].path, name),
        kind,
        generics: scope,
        region_params: lifetimes.len() as u32,
        variants,
        methods: Vec::new(),
        user_drop: false,
    }
}

fn collect_adt_methods<'a>(
    mid: ModIdx,
    adt_ref: AdtRef,
    agg: &'a ast::Aggregate,
    items: &mut Items<'a>,
    types: &mut Types,
    diags: &mut Vec<Diagnostic>,
) {
    let (host_generics, host_lifetimes) = type_generics(
        mid,
        &agg.generics,
        &items.modules,
        &items.mod_by_path,
        &items.adt_kinds,
        types,
        diags,
    );
    let host_generic_count = host_generics.len();
    let host_args: Vec<Ty> = (0..host_generic_count)
        .map(|i| types.intern(TyData::Param(i as u32)))
        .collect();
    let host_regions: Vec<Option<u32>> =
        (0..host_lifetimes.len()).map(|i| Some(i as u32)).collect();
    let self_ty = types.intern(TyData::Adt {
        adt: adt_ref,
        args: host_args,
        regions: host_regions,
    });
    let adt_name = items.adts[adt_ref.0 as usize].name.clone();
    for member in &agg.methods {
        match &member.kind {
            ast::ItemKind::Function(f) => {
                if items.adts[adt_ref.0 as usize]
                    .methods
                    .iter()
                    .any(|m| m.name == f.name)
                {
                    diags.push(diag::error(
                        diag::NAME_DUPLICATE,
                        member.span,
                        format!(
                            "method `{}` is declared more than once on `{adt_name}`",
                            f.name
                        ),
                    ));
                    continue;
                }
                if !f.contract.is_empty() {
                    let has_non_result = f.contract.iter().any(|c| c.name != "result");
                    if has_non_result {
                        diags.push(diag::unsupported("function contracts", f.span));
                    }
                }
                let (method_generics, method_lifetimes) = type_generics(
                    mid,
                    &f.generics,
                    &items.modules,
                    &items.mod_by_path,
                    &items.adt_kinds,
                    types,
                    diags,
                );
                let mut combined_generics = host_generics.clone();
                combined_generics.extend(method_generics);
                let mut combined_lifetimes = host_lifetimes.clone();
                combined_lifetimes.extend(method_lifetimes);
                let combined_scope: Vec<String> =
                    combined_generics.iter().map(|g| g.name.clone()).collect();
                let fn_ref = FnRef(items.fns.len() as u32);
                let mut params = Vec::with_capacity(f.params.len());
                for p in &f.params {
                    let ty = resolve_type_with_lifetimes(
                        &p.ty,
                        mid,
                        &combined_scope,
                        &combined_lifetimes,
                        &items.modules,
                        &items.mod_by_path,
                        &items.adt_kinds,
                        Some(self_ty),
                        Some(fn_ref),
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
                    Some(t) => resolve_type_with_lifetimes(
                        t,
                        mid,
                        &combined_scope,
                        &combined_lifetimes,
                        &items.modules,
                        &items.mod_by_path,
                        &items.adt_kinds,
                        Some(self_ty),
                        Some(fn_ref),
                        types,
                    )
                    .unwrap_or_else(|d| {
                        diags.push(d);
                        types.error()
                    }),
                    None => types.unit(),
                };
                let has_self = f.params.first().is_some_and(|p| p.name == "self");
                items.fns.push(FnDecl {
                    module: mid,
                    name: format!("{adt_name}::{}", f.name),
                    generics: combined_generics,
                    region_params: combined_lifetimes,
                    params,
                    result,
                    self_ty: Some(self_ty),
                    role: FnRoleSpec::Normal,
                    resume_mode: zil_ir::body::ResumeMode::Local,
                    state_contracts: Vec::new(),
                    ast: f,
                });
                items.adts[adt_ref.0 as usize]
                    .methods
                    .push(InlineMethodDecl {
                        name: f.name.clone(),
                        fn_ref,
                        public: member.public,
                        has_self,
                        host_generic_count,
                    });
            }
            ast::ItemKind::Import(_)
            | ast::ItemKind::Aggregate(_)
            | ast::ItemKind::Enum(_)
            | ast::ItemKind::Extern { .. }
            | ast::ItemKind::Impl(_)
            | ast::ItemKind::Trait(_)
            | ast::ItemKind::TypeAlias(_)
            | ast::ItemKind::Const(_)
            | ast::ItemKind::Meta(_) => {
                diags.push(diag::unsupported(
                    "non-function aggregate member",
                    member.span,
                ));
            }
        }
    }
}

enum ProtocolStateDomain<'a> {
    /// Built-in 3-state handles (`Task`, `Registration`, `AsyncRegistration`).
    Builtin,
    /// User-defined finite state protocol: either an `enum` directly or a
    /// `struct`/`package`/`resource` with an `enum`-typed `state` field.
    UserEnum {
        enum_short: &'a str,
        variants: &'a [VariantDecl],
    },
}

fn protocol_state_domain<'a>(
    ty: Ty,
    adts: &'a [AdtDecl],
    types: &Types,
) -> Option<ProtocolStateDomain<'a>> {
    let mut inner = ty;
    while let TyData::Ref { pointee, .. } | TyData::Managed(pointee) | TyData::Desc(pointee) =
        types.data(inner)
    {
        inner = *pointee;
    }
    match types.data(inner) {
        TyData::Opaque(
            OpaqueKind::Task { .. }
            | OpaqueKind::Registration(_)
            | OpaqueKind::AsyncRegistration(_),
        )
        | TyData::Param(_)
        | TyData::Error => Some(ProtocolStateDomain::Builtin),
        TyData::Adt { adt, .. } => {
            let decl = adts.get(adt.0 as usize)?;
            if decl.kind == AdtKind::Enum && !decl.variants.is_empty() && decl.variants.len() <= 64
            {
                let short = decl.name.rsplit('.').next().unwrap_or(&decl.name);
                return Some(ProtocolStateDomain::UserEnum {
                    enum_short: short,
                    variants: &decl.variants,
                });
            }
            if decl.variants.len() == 1
                && let Some(state_field) =
                    decl.variants[0].fields.iter().find(|f| f.name == "state")
                && let TyData::Adt { adt: state_adt, .. } = types.data(state_field.ty)
                && let Some(state_decl) = adts.get(state_adt.0 as usize)
                && state_decl.kind == AdtKind::Enum
                && !state_decl.variants.is_empty()
                && state_decl.variants.len() <= 64
            {
                let short = state_decl
                    .name
                    .rsplit('.')
                    .next()
                    .unwrap_or(&state_decl.name);
                return Some(ProtocolStateDomain::UserEnum {
                    enum_short: short,
                    variants: &state_decl.variants,
                });
            }
            None
        }
        TyData::Unit
        | TyData::Bool
        | TyData::Never
        | TyData::Int(_)
        | TyData::Float(_)
        | TyData::Tuple(_)
        | TyData::Union(_)
        | TyData::Array { .. }
        | TyData::ArrayConst { .. }
        | TyData::Slice(_)
        | TyData::Ref { .. }
        | TyData::Managed(_)
        | TyData::Desc(_)
        | TyData::ErrorUnion { .. }
        | TyData::FnPtr { .. }
        | TyData::Closure { .. }
        | TyData::ImplCallable { .. }
        | TyData::Dyn { .. }
        | TyData::OwnedDyn { .. }
        | TyData::SelfParam
        | TyData::Assoc { .. }
        | TyData::ConstParam(_)
        | TyData::ConstU32(_)
        | TyData::Type
        | TyData::Opaque(_)
        | TyData::Var(_) => None,
    }
}

fn parse_state_term(
    term: &ast::ContractTerm,
    params: &[ParamInfo],
    adts: &[AdtDecl],
    types: &Types,
    diags: &mut Vec<Diagnostic>,
) -> Option<Option<(u32, zil_ir::body::ProtocolStateSet)>> {
    if term.path.as_slice() != ["state"] || term.arguments.len() < 2 {
        return None;
    }
    let [pname] = term.arguments[0].path.as_slice() else {
        return None;
    };
    let Some(param_idx) = params.iter().position(|p| p.name == *pname) else {
        diags.push(diag::error(
            diag::RESOURCE_RESULT_MODE,
            term.arguments[0].span,
            format!(
                "contract `state(...)` must name a parameter of this function, found `{pname}`"
            ),
        ));
        return Some(None);
    };
    if !term.arguments[0].arguments.is_empty() {
        diags.push(diag::error(
            diag::RESOURCE_RESULT_MODE,
            term.arguments[0].span,
            format!(
                "contract `state(...)` must name a parameter of this function, found `{pname}`"
            ),
        ));
        return Some(None);
    }
    let Some(domain) = protocol_state_domain(params[param_idx].ty, adts, types) else {
        diags.push(diag::error(
            diag::RESOURCE_RESULT_MODE,
            params[param_idx].span,
            format!(
                "protocol state contract parameter `{pname}` must be a built-in protocol handle or a user struct/package/resource with an enum `state` field"
            ),
        ));
        return Some(None);
    };
    let mut mask: u64 = 0;
    let mut ok = true;
    for st in &term.arguments[1..] {
        let bit = match &domain {
            ProtocolStateDomain::Builtin => match st.path.as_slice() {
                [s] if st.arguments.is_empty() && s == "Open" => {
                    Some(zil_ir::body::ProtocolStateSet::OPEN)
                }
                [s] if st.arguments.is_empty() && s == "Closing" => {
                    Some(zil_ir::body::ProtocolStateSet::CLOSING)
                }
                [s] if st.arguments.is_empty() && s == "Closed" => {
                    Some(zil_ir::body::ProtocolStateSet::CLOSED)
                }
                _ => None,
            },
            ProtocolStateDomain::UserEnum {
                enum_short,
                variants,
            } => {
                if !st.arguments.is_empty() {
                    None
                } else {
                    let vname = match st.path.as_slice() {
                        [v] => Some(v.as_str()),
                        [en, v] if en == enum_short => Some(v.as_str()),
                        _ => None,
                    };
                    vname.and_then(|vn| {
                        variants
                            .iter()
                            .position(|v| v.name == vn)
                            .map(|idx| 1u64 << idx)
                    })
                }
            }
        };
        if let Some(b) = bit {
            mask |= b;
        } else {
            ok = false;
            match &domain {
                ProtocolStateDomain::Builtin => {
                    diags.push(diag::error(
                        diag::RESOURCE_RESULT_MODE,
                        st.span,
                        "contract `state(...)` only accepts `Open`, `Closing`, or `Closed`",
                    ));
                }
                ProtocolStateDomain::UserEnum { enum_short, .. } => {
                    diags.push(diag::error(
                        diag::RESOURCE_RESULT_MODE,
                        st.span,
                        format!(
                            "contract `state(...)` expects a variant of protocol state enum `{enum_short}`"
                        ),
                    ));
                }
            }
        }
    }
    if ok && mask != 0 {
        Some(Some((
            param_idx as u32,
            zil_ir::body::ProtocolStateSet(mask),
        )))
    } else {
        Some(None)
    }
}

fn parse_fn_contracts(
    clauses: &[ast::ContractClause],
    params: &[ParamInfo],
    adts: &[AdtDecl],
    types: &Types,
    diags: &mut Vec<Diagnostic>,
) -> (
    ResultMode,
    zil_ir::body::ResumeMode,
    Vec<zil_ir::body::StateContractClause>,
) {
    let mut result_mode = ResultMode::Default;
    let mut resume_mode: Option<zil_ir::body::ResumeMode> = None;
    let mut state_contracts: Vec<zil_ir::body::StateContractClause> = Vec::new();
    for c in clauses {
        if c.name == "result" {
            for term in &c.terms {
                let next = match term.path.as_slice() {
                    [s] if s == "create" && term.arguments.is_empty() => Some(ResultMode::Create),
                    [s] if s == "give" && term.arguments.is_empty() => Some(ResultMode::Give),
                    _ => None,
                };
                match next {
                    Some(m) if result_mode == ResultMode::Default || result_mode == m => {
                        result_mode = m
                    }
                    Some(_) => diags.push(diag::error(
                        diag::RESOURCE_RESULT_MODE,
                        term.span,
                        "`result(create)` and `result(give)` are mutually exclusive",
                    )),
                    None => diags.push(diag::unsupported("function contracts", term.span)),
                }
            }
        } else if c.name == "resume" {
            if c.terms.is_empty() {
                diags.push(diag::unsupported("function contracts", c.span));
            }
            for term in &c.terms {
                let next = match term.path.as_slice() {
                    [s] if s == "local" && term.arguments.is_empty() => {
                        Some(zil_ir::body::ResumeMode::Local)
                    }
                    [s] if s == "transfer" && term.arguments.is_empty() => {
                        Some(zil_ir::body::ResumeMode::Transfer)
                    }
                    _ => None,
                };
                match next {
                    Some(m) if resume_mode.is_none() || resume_mode == Some(m) => {
                        resume_mode = Some(m);
                    }
                    Some(_) => diags.push(diag::error(
                        diag::RESOURCE_RESULT_MODE,
                        term.span,
                        "`resume(local)` and `resume(transfer)` are mutually exclusive",
                    )),
                    None => diags.push(diag::unsupported("function contracts", term.span)),
                }
            }
        } else if c.name == "effects" {
            for term in &c.terms {
                if matches!(
                    term.path.as_slice(),
                    [s] if (s == "suspend" || s == "io") && term.arguments.is_empty()
                ) {
                    continue;
                }
                if term.path.as_slice() == ["invalidates"] {
                    if !state_contracts
                        .contains(&zil_ir::body::StateContractClause::InvalidationUpperBound)
                    {
                        state_contracts
                            .push(zil_ir::body::StateContractClause::InvalidationUpperBound);
                    }
                    for arg in &term.arguments {
                        if !arg.arguments.is_empty()
                            || arg.path.is_empty()
                            || arg.path.len() > 1 + zil_ir::body::ContractFieldPath::MAX_DEPTH
                        {
                            diags.push(diag::error(
                                diag::RESOURCE_RESULT_MODE,
                                arg.span,
                                "contract `invalidates(...)` target must be a parameter or dotted field path (max depth 4)",
                            ));
                            continue;
                        }
                        let Some(p_idx) = params.iter().position(|p| p.name == arg.path[0]) else {
                            diags.push(diag::error(
                                diag::RESOURCE_RESULT_MODE,
                                arg.span,
                                format!(
                                    "contract `invalidates(...)` references unknown parameter `{}`",
                                    arg.path[0]
                                ),
                            ));
                            continue;
                        };
                        let mut cur_ty = params[p_idx].ty;
                        let mut field_indices = Vec::new();
                        let mut ok = true;
                        for seg in &arg.path[1..] {
                            while let TyData::Ref { pointee, .. }
                            | TyData::Managed(pointee)
                            | TyData::Desc(pointee) = types.data(cur_ty)
                            {
                                cur_ty = *pointee;
                            }
                            if let TyData::Adt { adt, .. } = types.data(cur_ty)
                                && let Some(decl) = adts.get(adt.0 as usize)
                                && decl.variants.len() == 1
                                && let Some((fi, fdecl)) = decl.variants[0]
                                    .fields
                                    .iter()
                                    .enumerate()
                                    .find(|(_, f)| f.name == *seg)
                            {
                                field_indices.push(zil_ir::body::FieldIdx(fi as u32));
                                cur_ty = fdecl.ty;
                            } else {
                                diags.push(diag::error(
                                    diag::RESOURCE_RESULT_MODE,
                                    arg.span,
                                    format!(
                                        "contract `invalidates(...)` references unknown field `{seg}`"
                                    ),
                                ));
                                ok = false;
                                break;
                            }
                        }
                        if ok
                            && let Some(path) =
                                zil_ir::body::ContractFieldPath::from_slice(&field_indices)
                        {
                            state_contracts.push(zil_ir::body::StateContractClause::Invalidates {
                                param: p_idx as u32,
                                path,
                            });
                        }
                    }
                    continue;
                }
                if term.path.as_slice() == ["rollback"] && !term.arguments.is_empty() {
                    for arg in &term.arguments {
                        if let [pname] = arg.path.as_slice()
                            && arg.arguments.is_empty()
                            && let Some(p_idx) = params.iter().position(|p| p.name == *pname)
                        {
                            if protocol_state_domain(params[p_idx].ty, adts, types).is_none() {
                                diags.push(diag::error(
                                    diag::RESOURCE_RESULT_MODE,
                                    arg.span,
                                    format!(
                                        "contract `rollback({pname})` requires a protocol handle parameter"
                                    ),
                                ));
                            } else {
                                state_contracts.push(
                                    zil_ir::body::StateContractClause::AllowsRollback {
                                        param: p_idx as u32,
                                    },
                                );
                            }
                        } else {
                            diags.push(diag::error(
                                diag::RESOURCE_RESULT_MODE,
                                arg.span,
                                "contract `rollback(...)` must name a parameter of this function",
                            ));
                        }
                    }
                    continue;
                }
                diags.push(diag::unsupported("function effect clause", term.span));
            }
        } else if c.name == "requires" {
            for term in &c.terms {
                if term.path.as_slice() == ["apart"] && term.arguments.len() == 2 {
                    let a = &term.arguments[0];
                    let b = &term.arguments[1];
                    if let [aname] = a.path.as_slice()
                        && let [bname] = b.path.as_slice()
                        && a.arguments.is_empty()
                        && b.arguments.is_empty()
                        && let Some(pa) = params.iter().position(|p| p.name == *aname)
                        && let Some(pb) = params.iter().position(|p| p.name == *bname)
                    {
                        if pa == pb {
                            diags.push(diag::error(
                                diag::RESOURCE_RESULT_MODE,
                                term.span,
                                "contract `apart(a, b)` requires two distinct parameters",
                            ));
                        } else {
                            state_contracts.push(
                                zil_ir::body::StateContractClause::RequiresApart {
                                    param_a: pa as u32,
                                    param_b: pb as u32,
                                },
                            );
                        }
                    } else {
                        diags.push(diag::error(
                            diag::RESOURCE_RESULT_MODE,
                            term.span,
                            "contract `apart(a, b)` must name two parameters of this function",
                        ));
                    }
                    continue;
                }
                match parse_state_term(term, params, adts, types, diags) {
                    Some(Some((param, states))) => {
                        state_contracts.push(zil_ir::body::StateContractClause::RequiresState {
                            param,
                            states,
                        });
                    }
                    Some(None) => {}
                    None => diags.push(diag::unsupported("function contracts", term.span)),
                }
            }
        } else if c.name == "provides" {
            for term in &c.terms {
                if term.path.as_slice() == ["retired"] && term.arguments.len() == 1 {
                    let arg = &term.arguments[0];
                    if let [pname] = arg.path.as_slice()
                        && arg.arguments.is_empty()
                        && let Some(param_idx) = params.iter().position(|p| p.name == *pname)
                    {
                        if protocol_state_domain(params[param_idx].ty, adts, types).is_none() {
                            diags.push(diag::error(
                                diag::RESOURCE_RESULT_MODE,
                                params[param_idx].span,
                                format!(
                                    "protocol state contract parameter `{pname}` must be a protocol handle"
                                ),
                            ));
                        } else {
                            state_contracts.push(
                                zil_ir::body::StateContractClause::ProvidesRetired {
                                    param: param_idx as u32,
                                },
                            );
                        }
                        continue;
                    }
                    diags.push(diag::error(
                        diag::RESOURCE_RESULT_MODE,
                        arg.span,
                        "contract `retired(...)` must name a parameter of this function",
                    ));
                } else if term.path.as_slice() == ["state"]
                    && let Some(parsed) = parse_state_term(term, params, adts, types, diags)
                {
                    if let Some((param, states)) = parsed {
                        state_contracts.push(zil_ir::body::StateContractClause::ProvidesState {
                            param,
                            states,
                        });
                    }
                } else if matches!(
                    term.path.as_slice(),
                    [s] if s == "on_pending" || s == "on_ready" || s == "on_ok" || s == "on_err"
                ) && term.arguments.len() == 1
                    && let Some(parsed) =
                        parse_state_term(&term.arguments[0], params, adts, types, diags)
                {
                    if let Some((param, states)) = parsed {
                        let clause = match term.path[0].as_str() {
                            "on_pending" => zil_ir::body::StateContractClause::ProvidesOnPending {
                                param,
                                states,
                            },
                            "on_ready" => {
                                zil_ir::body::StateContractClause::ProvidesOnReady { param, states }
                            }
                            "on_ok" => {
                                zil_ir::body::StateContractClause::ProvidesOnOk { param, states }
                            }
                            _ => zil_ir::body::StateContractClause::ProvidesOnErr { param, states },
                        };
                        state_contracts.push(clause);
                    }
                } else {
                    diags.push(diag::unsupported("function contracts", term.span));
                }
            }
        } else {
            diags.push(diag::unsupported("function contracts", c.span));
        }
    }
    (
        result_mode,
        resume_mode.unwrap_or_default(),
        state_contracts,
    )
}

#[allow(clippy::too_many_arguments)]
fn collect_fn<'a>(
    mid: ModIdx,
    fn_ref: FnRef,
    f: &'a ast::Function,
    modules: &[ModuleScope],
    mod_by_path: &HashMap<String, ModIdx>,
    adt_kinds: &[AdtKind],
    adts: &[AdtDecl],
    types: &mut Types,
    diags: &mut Vec<Diagnostic>,
) -> FnDecl<'a> {
    let (mut generics, region_params) = type_generics(
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
        let _ = apply_where_clause(
            &f.where_clause,
            &mut generics,
            &scope,
            &region_params,
            mid,
            modules,
            mod_by_path,
            adt_kinds,
            types,
            diags,
        );
    }
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
    let (result_mode, resume_mode, state_contracts) =
        parse_fn_contracts(&f.contract, &params, adts, types, diags);
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
        self_ty: None,
        role: FnRoleSpec::Normal,
        resume_mode,
        state_contracts,
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

fn is_c_abi_pod_pointee(ty: Ty, types: &Types, adts: &[AdtDecl], seen: &mut Vec<AdtRef>) -> bool {
    if is_c_abi_param(ty, types) {
        return true;
    }
    if let TyData::Array { element, .. } = *types.data(ty) {
        return is_c_abi_pod_pointee(element, types, adts, seen);
    }
    if let TyData::Adt { adt, args, regions } = types.data(ty) {
        if !args.is_empty() || !regions.is_empty() {
            return false;
        }
        if seen.contains(adt) {
            return false;
        }
        let Some(decl) = adts.get(adt.0 as usize) else {
            return false;
        };
        if decl.kind != AdtKind::Record || decl.user_drop || decl.variants.len() != 1 {
            return false;
        }
        seen.push(*adt);
        let ok = decl.variants[0]
            .fields
            .iter()
            .all(|f| is_c_abi_pod_pointee(f.ty, types, adts, seen));
        seen.pop();
        return ok;
    }
    false
}

fn is_c_abi_noescape_param(ty: Ty, types: &Types, adts: &[AdtDecl]) -> bool {
    let TyData::Ref { pointee, .. } = *types.data(ty) else {
        return false;
    };
    let mut seen = Vec::new();
    is_c_abi_pod_pointee(pointee, types, adts, &mut seen)
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
    adts: &[AdtDecl],
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
    let mut callback_role: Option<zil_ir::body::ForeignCallbackRole> = None;
    let mut thread_async_driver_transfer = false;
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
                } else if term.arguments.is_empty() && term.path.as_slice() == ["static_callbacks"]
                {
                    continue;
                } else if term.arguments.is_empty()
                    && term.path.as_slice() == ["thread_async_driver_transfer"]
                {
                    thread_async_driver_transfer = true;
                } else if term.arguments.is_empty()
                    && let [s] = term.path.as_slice()
                    && let Some(role) = match s.as_str() {
                        "async_callback_register" => {
                            Some(zil_ir::body::ForeignCallbackRole::AsyncRegister)
                        }
                        "async_callback_request_close" => {
                            Some(zil_ir::body::ForeignCallbackRole::AsyncRequestClose)
                        }
                        "async_callback_poll_drain" => {
                            Some(zil_ir::body::ForeignCallbackRole::AsyncPollDrain)
                        }
                        "thread_async_callback_register" => {
                            Some(zil_ir::body::ForeignCallbackRole::ThreadAsyncRegister)
                        }
                        "thread_async_callback_request_close" => {
                            Some(zil_ir::body::ForeignCallbackRole::ThreadAsyncRequestClose)
                        }
                        "thread_async_callback_poll_drain" => {
                            Some(zil_ir::body::ForeignCallbackRole::ThreadAsyncPollDrain)
                        }
                        "scoped_callback_register" => {
                            Some(zil_ir::body::ForeignCallbackRole::ScopedRegister)
                        }
                        "scoped_callback_close" => {
                            Some(zil_ir::body::ForeignCallbackRole::ScopedClose)
                        }
                        _ => None,
                    }
                {
                    if let Some(prev) = callback_role
                        && prev != role
                    {
                        diags.push(diag::error(
                            diag::FFI_DECLARATION,
                            term.span,
                            "extern function cannot declare conflicting callback provider roles",
                        ));
                    } else {
                        callback_role = Some(role);
                    }
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
            && (is_c_abi_param(ty, types)
                || (noescape && is_c_abi_noescape_param(ty, types, adts)));
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
    if let Some(zil_ir::body::ForeignCallbackRole::ScopedRegister) = callback_role {
        let handle_ty = types.intern(TyData::Opaque(OpaqueKind::ForeignHandle));
        let i32_ty = types.intern(TyData::Int(IntTy::I32));
        let i64_ty = types.intern(TyData::Int(IntTy::I64));
        let cb_ty = types.intern(TyData::Opaque(OpaqueKind::ForeignCallback {
            params: vec![handle_ty, i32_ty],
            result: i64_ty,
        }));
        if params.as_slice() != [cb_ty, handle_ty] || result != handle_ty {
            diags.push(diag::error(
                diag::FFI_DECLARATION,
                span,
                "scoped_callback_register must have signature `extern \"C\" fn(extern \"C\" fn(c_handle, i32) -> i64, c_handle) -> c_handle`",
            ));
        }
    } else if let Some(zil_ir::body::ForeignCallbackRole::ScopedClose) = callback_role {
        let handle_ty = types.intern(TyData::Opaque(OpaqueKind::ForeignHandle));
        if params.as_slice() != [handle_ty] || !matches!(types.data(result), TyData::Unit) {
            diags.push(diag::error(
                diag::FFI_DECLARATION,
                span,
                "scoped_callback_close must have signature `extern \"C\" fn(c_handle)`",
            ));
        }
    }
    ForeignDeclInfo {
        name: f.name.clone(),
        symbol: f.name.clone(),
        params,
        noescape_params,
        callback_role,
        thread_async_driver_transfer,
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
    if let Some(t) = &imp.trait_type
        && let ast::TypeKind::Named { path, .. } = &t.kind
        && let [trait_name] = path.as_slice()
        && matches!(
            trait_name.as_str(),
            "Transfer" | "Share" | "Copy" | "SyncDrop" | "ClosedEnvironment" | "ThreadCallable"
        )
    {
        diags.push(diag::error(
            diag::GENERIC_BOUND,
            t.span,
            format!(
                "`{trait_name}` is a compiler-derived capability trait and cannot be explicitly implemented"
            ),
        ));
        return;
    }
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
        Some(
            ItemRef::Adt(_)
            | ItemRef::Fn(_)
            | ItemRef::Foreign(_)
            | ItemRef::Trait(_)
            | ItemRef::TypeAlias(_, _)
            | ItemRef::Const(_),
        )
        | None => {
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
        regions: Vec::new(),
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
        self_ty: Some(self_desc),
        role: FnRoleSpec::Drop(target_adt),
        resume_mode: zil_ir::body::ResumeMode::Local,
        state_contracts: Vec::new(),
        ast: f,
    });
}

fn inherent_target_adt(ty: Ty, types: &Types) -> Option<AdtRef> {
    match types.data(ty) {
        TyData::Adt { adt, .. } => Some(*adt),
        TyData::Desc(inner) => match types.data(*inner) {
            TyData::Adt { adt, .. } => Some(*adt),
            TyData::Unit
            | TyData::Type
            | TyData::ConstParam(_)
            | TyData::ConstU32(_)
            | TyData::Bool
            | TyData::Never
            | TyData::Int(_)
            | TyData::Float(_)
            | TyData::Tuple(_)
            | TyData::Array { .. }
            | TyData::ArrayConst { .. }
            | TyData::Slice(_)
            | TyData::Ref { .. }
            | TyData::Managed(_)
            | TyData::Desc(_)
            | TyData::Opaque(_)
            | TyData::ErrorUnion { .. }
            | TyData::FnPtr { .. }
            | TyData::Closure { .. }
            | TyData::ImplCallable { .. }
            | TyData::Dyn { .. }
            | TyData::OwnedDyn { .. }
            | TyData::Param(_)
            | TyData::SelfParam
            | TyData::Assoc { .. }
            | TyData::Var(_)
            | TyData::Union(_)
            | TyData::Error => None,
        },
        TyData::Unit
        | TyData::Type
        | TyData::ConstParam(_)
        | TyData::ConstU32(_)
        | TyData::Bool
        | TyData::Never
        | TyData::Int(_)
        | TyData::Float(_)
        | TyData::Tuple(_)
        | TyData::Union(_)
        | TyData::Array { .. }
        | TyData::ArrayConst { .. }
        | TyData::Slice(_)
        | TyData::Ref { .. }
        | TyData::Managed(_)
        | TyData::Opaque(_)
        | TyData::ErrorUnion { .. }
        | TyData::FnPtr { .. }
        | TyData::Closure { .. }
        | TyData::ImplCallable { .. }
        | TyData::Dyn { .. }
        | TyData::OwnedDyn { .. }
        | TyData::Param(_)
        | TyData::SelfParam
        | TyData::Assoc { .. }
        | TyData::Var(_)
        | TyData::Error => None,
    }
}

fn inherent_targets_overlap(a: Ty, b: Ty, types: &Types) -> bool {
    if a == b {
        return true;
    }
    let ua = match *types.data(a) {
        TyData::Desc(inner) => inner,
        TyData::Unit
        | TyData::Type
        | TyData::ConstParam(_)
        | TyData::ConstU32(_)
        | TyData::Bool
        | TyData::Never
        | TyData::Int(_)
        | TyData::Float(_)
        | TyData::Adt { .. }
        | TyData::Tuple(_)
        | TyData::Union(_)
        | TyData::Array { .. }
        | TyData::ArrayConst { .. }
        | TyData::Slice(_)
        | TyData::Ref { .. }
        | TyData::Managed(_)
        | TyData::Opaque(_)
        | TyData::ErrorUnion { .. }
        | TyData::FnPtr { .. }
        | TyData::Closure { .. }
        | TyData::ImplCallable { .. }
        | TyData::Dyn { .. }
        | TyData::OwnedDyn { .. }
        | TyData::Param(_)
        | TyData::SelfParam
        | TyData::Assoc { .. }
        | TyData::Var(_)
        | TyData::Error => a,
    };
    let ub = match *types.data(b) {
        TyData::Desc(inner) => inner,
        TyData::Unit
        | TyData::Type
        | TyData::ConstParam(_)
        | TyData::ConstU32(_)
        | TyData::Bool
        | TyData::Never
        | TyData::Int(_)
        | TyData::Float(_)
        | TyData::Adt { .. }
        | TyData::Tuple(_)
        | TyData::Union(_)
        | TyData::Array { .. }
        | TyData::ArrayConst { .. }
        | TyData::Slice(_)
        | TyData::Ref { .. }
        | TyData::Managed(_)
        | TyData::Opaque(_)
        | TyData::ErrorUnion { .. }
        | TyData::FnPtr { .. }
        | TyData::Closure { .. }
        | TyData::ImplCallable { .. }
        | TyData::Dyn { .. }
        | TyData::OwnedDyn { .. }
        | TyData::Param(_)
        | TyData::SelfParam
        | TyData::Assoc { .. }
        | TyData::Var(_)
        | TyData::Error => b,
    };
    if ua == ub {
        return true;
    }
    if let (
        TyData::Adt {
            adt: a1,
            args: args1,
            ..
        },
        TyData::Adt {
            adt: a2,
            args: args2,
            ..
        },
    ) = (types.data(ua), types.data(ub))
    {
        return a1 == a2
            && args1.len() == args2.len()
            && args1.iter().zip(args2.iter()).all(|(&x, &y)| {
                x == y
                    || matches!(types.data(x), TyData::Param(_))
                    || matches!(types.data(y), TyData::Param(_))
            });
    }
    false
}

fn inherent_target_label(ty: Ty, items: &Items<'_>, types: &Types) -> String {
    match types.data(ty) {
        TyData::Adt { adt, .. } => items.adts[adt.0 as usize].name.clone(),
        TyData::Desc(inner) => inherent_target_label(*inner, items, types),
        TyData::Int(it) => format!("{it:?}").to_lowercase(),
        TyData::Float(ft) => format!("{ft:?}").to_lowercase(),
        TyData::Bool => "bool".to_string(),
        TyData::Unit => "void".to_string(),
        TyData::Type
        | TyData::ConstParam(_)
        | TyData::ConstU32(_)
        | TyData::Never
        | TyData::Tuple(_)
        | TyData::Array { .. }
        | TyData::ArrayConst { .. }
        | TyData::Slice(_)
        | TyData::Ref { .. }
        | TyData::Managed(_)
        | TyData::Opaque(_)
        | TyData::ErrorUnion { .. }
        | TyData::FnPtr { .. }
        | TyData::Closure { .. }
        | TyData::ImplCallable { .. }
        | TyData::Dyn { .. }
        | TyData::OwnedDyn { .. }
        | TyData::Param(_)
        | TyData::SelfParam
        | TyData::Assoc { .. }
        | TyData::Var(_)
        | TyData::Union(_)
        | TyData::Error => "impl".to_string(),
    }
}

fn collect_inherent_impl<'a>(
    mid: ModIdx,
    imp: &'a ast::ImplBlock,
    items: &mut Items<'a>,
    types: &mut Types,
    diags: &mut Vec<Diagnostic>,
) {
    let (mut impl_generics, impl_lifetimes) = type_generics(
        mid,
        &imp.generics,
        &items.modules,
        &items.mod_by_path,
        &items.adt_kinds,
        types,
        diags,
    );
    let scope: Vec<String> = impl_generics.iter().map(|g| g.name.clone()).collect();
    for clause in &imp.where_clause {
        match &clause.subject {
            ast::BoundSubject::Type(sub_ty) => {
                if let ast::TypeKind::Named { path, args } = &sub_ty.kind
                    && args.is_empty()
                    && let [pname] = path.as_slice()
                    && let Some(g_idx) = impl_generics.iter().position(|g| g.name == *pname)
                {
                    for b in &clause.bounds {
                        if let Some(c) = resolve_callable_bound(
                            b,
                            mid,
                            &scope,
                            &items.modules,
                            &items.mod_by_path,
                            &items.adt_kinds,
                            types,
                            diags,
                        ) {
                            impl_generics[g_idx].callable = Some(c);
                            continue;
                        }
                        if let ast::TypeKind::Named { path, args } = &b.kind {
                            if args.is_empty() && path == &["Copy"] {
                                impl_generics[g_idx].copy = true;
                            } else if args.is_empty() && path == &["StoreElement"] {
                                impl_generics[g_idx].store_element = true;
                            } else if args.is_empty() && path == &["SyncDrop"] {
                                impl_generics[g_idx].sync_drop = true;
                            } else if args.is_empty()
                                && matches!(
                                    path.as_slice(),
                                    [s] if matches!(
                                        s.as_str(),
                                        "Value"
                                            | "ClosedEnvironment"
                                            | "Transfer"
                                            | "Share"
                                            | "ThreadCallable"
                                    )
                                )
                            {
                                continue;
                            } else if let Some(ItemRef::Trait(tr)) =
                                lookup_item_in(&items.modules, &items.mod_by_path, mid, path)
                            {
                                let mut targs = Vec::new();
                                for a in args {
                                    if let ast::GenericArg::Type(t) = a {
                                        targs.push(
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
                                        diags.push(diag::unsupported(
                                            "non-type trait bound argument",
                                            b.span,
                                        ));
                                    }
                                }
                                impl_generics[g_idx].traits.push(TraitBound {
                                    trait_ref: tr,
                                    trait_args: targs,
                                    assoc_bindings: Vec::new(),
                                });
                            } else {
                                diags.push(diag::error(
                                    diag::GENERIC_BOUND,
                                    b.span,
                                    format!("unknown bound `{}`", path.join(".")),
                                ));
                            }
                        } else {
                            diags.push(diag::unsupported("non-named generic bound", b.span));
                        }
                    }
                } else {
                    diags.push(diag::unsupported(
                        "where clause on non-parameter type",
                        clause.span,
                    ));
                }
            }
            ast::BoundSubject::Lifetime(_) => {
                diags.push(diag::unsupported(
                    "lifetime where clause on inherent impl",
                    clause.span,
                ));
            }
        }
    }
    let target = resolve_type_with_lifetimes(
        &imp.target,
        mid,
        &scope,
        &impl_lifetimes,
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
    match types.data(target) {
        TyData::Error => return,
        TyData::Adt { .. }
        | TyData::Desc(_)
        | TyData::Int(_)
        | TyData::Float(_)
        | TyData::Bool
        | TyData::Unit => {}
        TyData::Type
        | TyData::ConstParam(_)
        | TyData::ConstU32(_)
        | TyData::Never
        | TyData::Tuple(_)
        | TyData::Array { .. }
        | TyData::ArrayConst { .. }
        | TyData::Slice(_)
        | TyData::Ref { .. }
        | TyData::Managed(_)
        | TyData::Opaque(_)
        | TyData::ErrorUnion { .. }
        | TyData::FnPtr { .. }
        | TyData::Closure { .. }
        | TyData::ImplCallable { .. }
        | TyData::Dyn { .. }
        | TyData::OwnedDyn { .. }
        | TyData::Param(_)
        | TyData::SelfParam
        | TyData::Assoc { .. }
        | TyData::Var(_)
        | TyData::Union(_) => {
            diags.push(diag::unsupported(
                "inherent impl for this target type",
                imp.target.span,
            ));
            return;
        }
    }
    let target_label = inherent_target_label(target, items, types);
    let target_adt = inherent_target_adt(target, types);
    let impl_generic_count = impl_generics.len();
    let mut methods: Vec<InherentMethodDecl> = Vec::new();
    for m in &imp.members {
        match &m.kind {
            ast::ItemKind::Function(f) => {
                let dup_in_block = methods.iter().any(|em| em.name == f.name);
                let dup_in_inline = target_adt.is_some_and(|adt| {
                    items.adts[adt.0 as usize]
                        .methods
                        .iter()
                        .any(|em| em.name == f.name)
                });
                let dup_in_other = items.inherent_impls.iter().any(|existing| {
                    inherent_targets_overlap(existing.target, target, types)
                        && existing.methods.iter().any(|em| em.name == f.name)
                });
                if dup_in_block || dup_in_inline || dup_in_other {
                    diags.push(diag::error(
                        diag::NAME_DUPLICATE,
                        m.span,
                        format!(
                            "method `{}` is declared more than once on `{target_label}`",
                            f.name
                        ),
                    ));
                    continue;
                }
                let mut result_mode = ResultMode::Default;
                for c in &f.contract {
                    if c.name == "result" {
                        for term in &c.terms {
                            let next = match term.path.as_slice() {
                                [s] if s == "create" && term.arguments.is_empty() => {
                                    Some(ResultMode::Create)
                                }
                                [s] if s == "give" && term.arguments.is_empty() => {
                                    Some(ResultMode::Give)
                                }
                                _ => None,
                            };
                            match next {
                                Some(m)
                                    if result_mode == ResultMode::Default || result_mode == m =>
                                {
                                    result_mode = m;
                                }
                                Some(_) => diags.push(diag::error(
                                    diag::RESOURCE_RESULT_MODE,
                                    term.span,
                                    "`result(create)` and `result(give)` are mutually exclusive",
                                )),
                                None => {
                                    diags.push(diag::unsupported("function contracts", term.span));
                                }
                            }
                        }
                    } else {
                        diags.push(diag::unsupported("function contracts", f.span));
                    }
                }
                let (method_generics, method_lifetimes) = type_generics(
                    mid,
                    &f.generics,
                    &items.modules,
                    &items.mod_by_path,
                    &items.adt_kinds,
                    types,
                    diags,
                );
                let mut combined_generics = impl_generics.clone();
                combined_generics.extend(method_generics);
                let mut combined_lifetimes = impl_lifetimes.clone();
                combined_lifetimes.extend(method_lifetimes);
                let combined_scope: Vec<String> =
                    combined_generics.iter().map(|g| g.name.clone()).collect();
                let fn_ref = FnRef(items.fns.len() as u32);
                let mut params = Vec::with_capacity(f.params.len() + 1);
                if f.params.first().map(|p| p.name.as_str()) != Some("self")
                    && let Some(recv_p) = &imp.receiver_default
                {
                    let recv_ty = resolve_type_with_lifetimes(
                        &recv_p.ty,
                        mid,
                        &combined_scope,
                        &combined_lifetimes,
                        &items.modules,
                        &items.mod_by_path,
                        &items.adt_kinds,
                        Some(target),
                        Some(fn_ref),
                        types,
                    )
                    .unwrap_or_else(|d| {
                        diags.push(d);
                        types.error()
                    });
                    params.push(ParamInfo {
                        name: "self".into(),
                        ty: recv_ty,
                        span: recv_p.span,
                    });
                }
                for p in &f.params {
                    let ty = resolve_type_with_lifetimes(
                        &p.ty,
                        mid,
                        &combined_scope,
                        &combined_lifetimes,
                        &items.modules,
                        &items.mod_by_path,
                        &items.adt_kinds,
                        Some(target),
                        Some(fn_ref),
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
                let raw_result = match &f.return_type {
                    Some(t) => resolve_type_with_lifetimes(
                        t,
                        mid,
                        &combined_scope,
                        &combined_lifetimes,
                        &items.modules,
                        &items.mod_by_path,
                        &items.adt_kinds,
                        Some(target),
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
                let has_self = params.first().is_some_and(|p| p.name == "self");
                items.fns.push(FnDecl {
                    module: mid,
                    name: format!("{target_label}::{}", f.name),
                    generics: combined_generics,
                    region_params: combined_lifetimes,
                    params,
                    result,
                    self_ty: Some(target),
                    role: FnRoleSpec::Normal,
                    resume_mode: zil_ir::body::ResumeMode::Local,
                    state_contracts: Vec::new(),
                    ast: f,
                });
                methods.push(InherentMethodDecl {
                    name: f.name.clone(),
                    fn_ref,
                    public: m.public,
                    has_self,
                    impl_generic_count,
                    module: mid,
                });
            }
            ast::ItemKind::Import(_)
            | ast::ItemKind::Aggregate(_)
            | ast::ItemKind::Enum(_)
            | ast::ItemKind::Extern { .. }
            | ast::ItemKind::Impl(_)
            | ast::ItemKind::Trait(_)
            | ast::ItemKind::TypeAlias(_)
            | ast::ItemKind::Const(_)
            | ast::ItemKind::Meta(_) => {
                diags.push(diag::unsupported("non-function impl member", m.span));
            }
        }
    }
    items.inherent_impls.push(InherentImplDecl {
        generics: impl_generics,
        target,
        methods,
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
        collect_inherent_impl(mid, imp, items, types, diags);
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
    let (mut impl_generics, impl_lifetimes) = type_generics(
        mid,
        &imp.generics,
        &items.modules,
        &items.mod_by_path,
        &items.adt_kinds,
        types,
        diags,
    );
    let scope: Vec<String> = impl_generics.iter().map(|g| g.name.clone()).collect();
    let where_bounds = if !imp.where_clause.is_empty() {
        apply_where_clause(
            &imp.where_clause,
            &mut impl_generics,
            &scope,
            &impl_lifetimes,
            mid,
            &items.modules,
            &items.mod_by_path,
            &items.adt_kinds,
            types,
            diags,
        )
    } else {
        Vec::new()
    };
    let mut trait_args = Vec::new();
    for a in args {
        match a {
            ast::GenericArg::Type(t) => {
                trait_args.push(
                    resolve_type_with_lifetimes(
                        t,
                        mid,
                        &scope,
                        &impl_lifetimes,
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
            }
            ast::GenericArg::Lifetime(_) => {}
            ast::GenericArg::Const(_) | ast::GenericArg::AssociatedBinding(_) => {
                diags.push(diag::unsupported("non-type trait arguments", trait_ty.span));
            }
        }
    }
    let target = resolve_type_with_lifetimes(
        &imp.target,
        mid,
        &scope,
        &impl_lifetimes,
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
    let trait_assoc_names = items.traits[trait_ref.0 as usize].assoc_types.clone();
    let mut assoc_types = Vec::new();
    let mut methods = Vec::new();
    for m in &imp.members {
        match &m.kind {
            ast::ItemKind::TypeAlias(ta) => {
                if let Some(val) = &ta.value {
                    let ty = resolve_type_with_lifetimes(
                        val,
                        mid,
                        &scope,
                        &impl_lifetimes,
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
                let (m_generics, m_lifetimes) = type_generics_in_scope(
                    mid,
                    &f.generics,
                    &scope,
                    &impl_lifetimes,
                    &items.modules,
                    &items.mod_by_path,
                    &items.adt_kinds,
                    types,
                    diags,
                );
                let mut fn_generics = impl_generics.clone();
                fn_generics.extend(m_generics);
                let mut fn_scope = scope.clone();
                fn_scope.extend(
                    fn_generics[impl_generics.len()..]
                        .iter()
                        .map(|g| g.name.clone()),
                );
                let mut fn_lifetimes = impl_lifetimes.clone();
                fn_lifetimes.extend(m_lifetimes);
                if !f.where_clause.is_empty() {
                    let _ = apply_where_clause(
                        &f.where_clause,
                        &mut fn_generics,
                        &fn_scope,
                        &fn_lifetimes,
                        mid,
                        &items.modules,
                        &items.mod_by_path,
                        &items.adt_kinds,
                        types,
                        diags,
                    );
                }
                let mut params = Vec::new();
                if f.params.first().map(|p| p.name.as_str()) != Some("self")
                    && let Some(tm) = items
                        .traits
                        .get(trait_ref.0 as usize)
                        .and_then(|tr| tr.methods.iter().find(|m| m.name == f.name))
                    && tm.params.len() == f.params.len() + 1
                {
                    let mut subst_vec = trait_args.clone();
                    for j in 0..tm.generics.len() {
                        subst_vec
                            .push(types.intern(TyData::Param((impl_generics.len() + j) as u32)));
                    }
                    let self_param_ty = types.subst_with_self(tm.params[0], &subst_vec, target);
                    params.push(ParamInfo {
                        name: "self".into(),
                        ty: self_param_ty,
                        span: f.span,
                    });
                }
                for p in &f.params {
                    let ty = resolve_type_with_lifetimes(
                        &p.ty,
                        mid,
                        &fn_scope,
                        &fn_lifetimes,
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
                    let ty = qualify_self_assoc(ty, target, trait_ref, &trait_assoc_names, types);
                    params.push(ParamInfo {
                        name: p.name.clone(),
                        ty,
                        span: p.span,
                    });
                }
                let result = match &f.return_type {
                    Some(t) => {
                        let ty = resolve_type_with_lifetimes(
                            t,
                            mid,
                            &fn_scope,
                            &fn_lifetimes,
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
                        qualify_self_assoc(ty, target, trait_ref, &trait_assoc_names, types)
                    }
                    None => types.unit(),
                };
                let fn_ref = FnRef(items.fns.len() as u32);
                items.fns.push(FnDecl {
                    module: mid,
                    name: format!("{trait_name}::{}", f.name),
                    generics: fn_generics,
                    region_params: fn_lifetimes,
                    params,
                    result,
                    self_ty: Some(target),
                    role: FnRoleSpec::Normal,
                    resume_mode: zil_ir::body::ResumeMode::Local,
                    state_contracts: Vec::new(),
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
    let trait_module = items.traits[trait_ref.0 as usize].module;
    let trait_defaults: Vec<(TraitMethodDecl<'a>, &'a ast::Function)> = items.traits
        [trait_ref.0 as usize]
        .methods
        .iter()
        .filter_map(|tm| {
            let default_f = tm.default_ast?;
            if methods.iter().any(|(n, _)| n == &tm.name) {
                return None;
            }
            Some((tm.clone(), default_f))
        })
        .collect();
    for (tm, default_f) in trait_defaults {
        let mname = tm.name;
        let tm_generics = tm.generics;
        let tm_regions = tm.region_params;
        let tm_params = tm.params;
        let tm_result = tm.result;
        let mut subst_vec = trait_args.clone();
        for j in 0..tm_generics.len() {
            subst_vec.push(types.intern(TyData::Param((impl_generics.len() + j) as u32)));
        }
        let mut fn_generics = impl_generics.clone();
        for mut mg in tm_generics {
            for b in &mut mg.traits {
                b.trait_args = b
                    .trait_args
                    .iter()
                    .map(|t| types.subst_with_self(*t, &subst_vec, target))
                    .collect();
                b.assoc_bindings = b
                    .assoc_bindings
                    .iter()
                    .map(|(n, t)| (n.clone(), types.subst_with_self(*t, &subst_vec, target)))
                    .collect();
            }
            fn_generics.push(mg);
        }
        let mut fn_regions = impl_lifetimes.clone();
        fn_regions.extend(tm_regions);
        let mut params = Vec::with_capacity(tm_params.len());
        for (idx, &pty) in tm_params.iter().enumerate() {
            let subst_ty = types.subst_with_self(pty, &subst_vec, target);
            let (pname, pspan) = default_f
                .params
                .get(idx)
                .map(|p| (p.name.clone(), p.span))
                .unwrap_or_else(|| ("self".into(), default_f.span));
            params.push(ParamInfo {
                name: pname,
                ty: subst_ty,
                span: pspan,
            });
        }
        let result = types.subst_with_self(tm_result, &subst_vec, target);
        let fn_ref = FnRef(items.fns.len() as u32);
        items.fns.push(FnDecl {
            module: trait_module,
            name: format!("{trait_name}::{mname}"),
            generics: fn_generics,
            region_params: fn_regions,
            params,
            result,
            self_ty: Some(target),
            role: FnRoleSpec::Normal,
            resume_mode: zil_ir::body::ResumeMode::Local,
            state_contracts: Vec::new(),
            ast: default_f,
        });
        methods.push((mname, fn_ref));
    }
    items.trait_impls.push(TraitImplDecl {
        trait_ref,
        generics: impl_generics,
        where_bounds,
        trait_args,
        target,
        assoc_types,
        methods,
        span,
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
    resolve_type_with_self(t, from, generics, lifetimes, None, items, types)
}

#[allow(clippy::result_large_err)] // `Diagnostic` is the workspace-wide error type.
pub fn resolve_type_with_self(
    t: &ast::TypeRef,
    from: ModIdx,
    generics: &[String],
    lifetimes: &[String],
    self_ty: Option<Ty>,
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
        self_ty,
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

/// Attaches `trait_ref` to unqualified `Assoc { base: self_ty, trait_ref: None, name }`
/// projections when `name` is declared directly on `trait_ref`, so normalization
/// disambiguates associated types of the same name across multiple traits.
fn qualify_self_assoc(
    ty: Ty,
    self_ty: Ty,
    trait_ref: TraitRef,
    local_assoc_types: &[String],
    types: &mut Types,
) -> Ty {
    if local_assoc_types.is_empty() {
        return ty;
    }
    match types.data(ty).clone() {
        TyData::Assoc {
            base,
            trait_ref: existing,
            name,
        } => {
            let base = qualify_self_assoc(base, self_ty, trait_ref, local_assoc_types, types);
            let qualified = if existing.is_none()
                && base == self_ty
                && local_assoc_types.iter().any(|a| a == &name)
            {
                Some(trait_ref)
            } else {
                existing
            };
            types.intern(TyData::Assoc {
                base,
                trait_ref: qualified,
                name,
            })
        }
        TyData::Adt { adt, args, regions } => {
            let args = args
                .into_iter()
                .map(|a| qualify_self_assoc(a, self_ty, trait_ref, local_assoc_types, types))
                .collect();
            types.intern(TyData::Adt { adt, args, regions })
        }
        TyData::Tuple(items) => {
            let items = items
                .into_iter()
                .map(|a| qualify_self_assoc(a, self_ty, trait_ref, local_assoc_types, types))
                .collect();
            types.intern(TyData::Tuple(items))
        }
        TyData::Array { element, length } => {
            let element = qualify_self_assoc(element, self_ty, trait_ref, local_assoc_types, types);
            types.intern(TyData::Array { element, length })
        }
        TyData::ArrayConst { element, length } => {
            let element = qualify_self_assoc(element, self_ty, trait_ref, local_assoc_types, types);
            types.intern(TyData::ArrayConst { element, length })
        }
        TyData::Slice(element) => {
            let element = qualify_self_assoc(element, self_ty, trait_ref, local_assoc_types, types);
            types.intern(TyData::Slice(element))
        }
        TyData::Ref {
            region,
            mutable,
            pointee,
        } => {
            let pointee = qualify_self_assoc(pointee, self_ty, trait_ref, local_assoc_types, types);
            types.intern(TyData::Ref {
                region,
                mutable,
                pointee,
            })
        }
        TyData::Managed(inner) => {
            let inner = qualify_self_assoc(inner, self_ty, trait_ref, local_assoc_types, types);
            types.intern(TyData::Managed(inner))
        }
        TyData::Desc(inner) => {
            let inner = qualify_self_assoc(inner, self_ty, trait_ref, local_assoc_types, types);
            types.intern(TyData::Desc(inner))
        }
        TyData::ErrorUnion { ok, err } => {
            let ok = qualify_self_assoc(ok, self_ty, trait_ref, local_assoc_types, types);
            let err = qualify_self_assoc(err, self_ty, trait_ref, local_assoc_types, types);
            types.intern(TyData::ErrorUnion { ok, err })
        }
        TyData::FnPtr { params, result } => {
            let params = params
                .into_iter()
                .map(|a| qualify_self_assoc(a, self_ty, trait_ref, local_assoc_types, types))
                .collect();
            let result = qualify_self_assoc(result, self_ty, trait_ref, local_assoc_types, types);
            types.intern(TyData::FnPtr { params, result })
        }
        TyData::Union(items) => {
            let items = items
                .into_iter()
                .map(|a| qualify_self_assoc(a, self_ty, trait_ref, local_assoc_types, types))
                .collect();
            types.canonicalize_union_raw(items)
        }
        TyData::Unit
        | TyData::Type
        | TyData::ConstParam(_)
        | TyData::ConstU32(_)
        | TyData::Bool
        | TyData::Never
        | TyData::Int(_)
        | TyData::Float(_)
        | TyData::Opaque(_)
        | TyData::Closure { .. }
        | TyData::ImplCallable { .. }
        | TyData::Dyn { .. }
        | TyData::OwnedDyn { .. }
        | TyData::Param(_)
        | TyData::SelfParam
        | TyData::Var(_)
        | TyData::Error => ty,
    }
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
                        trait_ref: None,
                        name: assoc.clone(),
                    }));
                }
                if let Some(i) = generics.iter().rposition(|g| g == base) {
                    let base_ty = types.intern(TyData::Param(i as u32));
                    return Ok(types.intern(TyData::Assoc {
                        base: base_ty,
                        trait_ref: None,
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
                if let Some(i) = generics.iter().rposition(|g| g == name) {
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
                    let (inner_ast, ctx_ast) = match args.as_slice() {
                        [ast::GenericArg::Type(inner_ast)] => (inner_ast, None),
                        [
                            ast::GenericArg::Type(inner_ast),
                            ast::GenericArg::Type(ctx_ast),
                        ] => (inner_ast, Some(ctx_ast)),
                        _ => {
                            return Err(diag::error(
                                diag::TYPE_ARGUMENT,
                                t.span,
                                "`OwnedDyn<dyn Trait[, Context]>` takes one or two type arguments",
                            ));
                        }
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
                    let TyData::Dyn {
                        trait_ref: tr,
                        trait_args,
                        assoc_bindings,
                    } = types.data(inner).clone()
                    else {
                        return Err(diag::error(
                            diag::TYPE_ARGUMENT,
                            t.span,
                            "`OwnedDyn` first argument must be a `dyn Trait` type",
                        ));
                    };
                    let context = match ctx_ast {
                        Some(c_ast) => Some(resolve_type_with_lifetimes(
                            c_ast,
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
                        None => None,
                    };
                    return Ok(types.intern(TyData::OwnedDyn {
                        trait_ref: tr,
                        trait_args,
                        assoc_bindings,
                        context,
                    }));
                }
                if matches!(
                    name.as_str(),
                    "Task" | "Mutex" | "MutexGuard" | "Registration" | "AsyncRegistration"
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
                            let resource_inner = if let TyData::Desc(inner) = types.data(elem) {
                                Some(*inner)
                            } else if matches!(types.data(elem), TyData::Adt { .. }) {
                                Some(elem)
                            } else {
                                None
                            };
                            let payload = if let Some(r_inner) = resource_inner
                                && let TyData::Adt { adt, .. } = types.data(r_inner)
                                && adt_kinds
                                    .get(adt.0 as usize)
                                    .is_some_and(|k| *k == AdtKind::Resource)
                            {
                                types.intern(TyData::Managed(r_inner))
                            } else {
                                elem
                            };
                            let inner = types.intern(TyData::Opaque(OpaqueKind::Mutex(payload)));
                            Ok(types.intern(TyData::Desc(inner)))
                        }
                        "MutexGuard" => {
                            Ok(types.intern(TyData::Opaque(OpaqueKind::MutexGuard(elem))))
                        }
                        "Registration" => {
                            let inner =
                                types.intern(TyData::Opaque(OpaqueKind::Registration(elem)));
                            Ok(types.intern(TyData::Desc(inner)))
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
                    let mut regions = Vec::new();
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
                            ast::GenericArg::Lifetime(lt) => {
                                let r = lifetimes.iter().rposition(|l| l == lt).map(|i| i as u32);
                                regions.push(r);
                            }
                            ast::GenericArg::Const(expr) => {
                                if let Some(n) = eval_const_u32(expr, from, modules, mod_by_path) {
                                    tys.push(types.intern(TyData::ConstU32(n)));
                                } else if let ast::ExprKind::Name(cname) = &expr.kind
                                    && let Some(idx) = generics.iter().rposition(|g| g == cname)
                                {
                                    tys.push(types.intern(TyData::ConstParam(idx as u32)));
                                } else {
                                    return Err(diag::error(
                                        diag::ARRAY_LENGTH,
                                        expr.span,
                                        "const generic argument must be a compile-time usize constant",
                                    ));
                                }
                            }
                            ast::GenericArg::AssociatedBinding(_) => {
                                return Err(diag::unsupported(
                                    "non-type generic arguments",
                                    t.span,
                                ));
                            }
                        }
                    }
                    let adt_ty = types.intern(TyData::Adt {
                        adt,
                        args: tys,
                        regions,
                    });
                    if adt_kinds[adt.0 as usize] == AdtKind::Resource {
                        Ok(types.intern(TyData::Desc(adt_ty)))
                    } else {
                        Ok(adt_ty)
                    }
                }
                Some(ItemRef::TypeAlias(ta_mod, ta_idx)) => {
                    let (arity, target) = modules[ta_mod.0 as usize].type_aliases[ta_idx as usize];
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
                            ast::GenericArg::Const(expr) => {
                                if let Some(n) = eval_const_u32(expr, from, modules, mod_by_path) {
                                    tys.push(types.intern(TyData::ConstU32(n)));
                                } else if let ast::ExprKind::Name(cname) = &expr.kind
                                    && let Some(idx) = generics.iter().rposition(|g| g == cname)
                                {
                                    tys.push(types.intern(TyData::ConstParam(idx as u32)));
                                } else {
                                    return Err(diag::error(
                                        diag::ARRAY_LENGTH,
                                        expr.span,
                                        "const generic argument must be a compile-time usize constant",
                                    ));
                                }
                            }
                            ast::GenericArg::AssociatedBinding(_) => {
                                return Err(diag::unsupported(
                                    "non-type generic arguments",
                                    t.span,
                                ));
                            }
                        }
                    }
                    if tys.len() != arity {
                        return Err(diag::error(
                            diag::TYPE_ARGUMENT,
                            t.span,
                            format!(
                                "type alias expects {arity} type argument(s), found {}",
                                tys.len()
                            ),
                        ));
                    }
                    Ok(if tys.is_empty() {
                        target
                    } else {
                        types.subst(target, &tys)
                    })
                }
                Some(
                    ItemRef::Fn(_) | ItemRef::Foreign(_) | ItemRef::Trait(_) | ItemRef::Const(_),
                )
                | None => Err(diag::error(
                    diag::NAME_UNRESOLVED,
                    t.span,
                    format!("unknown type `{}`", path.join(".")),
                )),
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
            let n = eval_const_u32(length, from, modules, mod_by_path);
            if let Some(length) = n {
                return Ok(types.intern(TyData::Array { element, length }));
            }
            if let ast::ExprKind::Name(cname) = &length.kind
                && let Some(idx) = generics.iter().rposition(|g| g == cname)
            {
                let len_ty = types.intern(TyData::ConstParam(idx as u32));
                return Ok(types.intern(TyData::ArrayConst {
                    element,
                    length: len_ty,
                }));
            }
            Err(diag::error(
                diag::ARRAY_LENGTH,
                t.span,
                "array length must be a usize literal or const generic parameter",
            ))
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
                    TyData::Opaque(
                        OpaqueKind::Task { .. }
                            | OpaqueKind::Registration(_)
                            | OpaqueKind::AsyncRegistration(_),
                    )
                )
            {
                pointee = types.intern(TyData::Managed(inner_ty));
            }
            let region = lifetime.as_deref().and_then(|lt| {
                let name = lt.strip_prefix('\'').unwrap_or(lt);
                lifetimes.iter().rposition(|r| r == name).map(|i| i as u32)
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
                regions: Vec::new(),
            }))
        }
        ast::TypeKind::Union(members) => {
            let mut resolved = Vec::with_capacity(members.len());
            for m in members {
                resolved.push(resolve_type_with_lifetimes(
                    m,
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
            match types.make_union(resolved) {
                Ok(ty) => Ok(ty),
                Err(crate::ty::UnionError::TooFew) => Err(diag::error(
                    diag::TYPE_MISMATCH,
                    t.span,
                    "anonymous union requires at least two distinct member types",
                )),
                Err(crate::ty::UnionError::Duplicate(_)) => Err(diag::error(
                    diag::TYPE_MISMATCH,
                    t.span,
                    "anonymous union contains duplicate member types",
                )),
            }
        }
        ast::TypeKind::Dyn {
            path,
            args,
            bindings,
        } => {
            if let Some(ItemRef::Trait(tr)) = lookup_item_in(modules, mod_by_path, from, path) {
                let mut trait_args = Vec::new();
                let mut assoc_bindings = Vec::new();
                for a in args {
                    match a {
                        ast::GenericArg::Type(arg_ty) => {
                            trait_args.push(resolve_type_with_lifetimes(
                                arg_ty,
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
                        ast::GenericArg::AssociatedBinding(binding) => match &binding.kind {
                            ast::DynBindingKind::Type { member, value } if member.len() == 1 => {
                                let val_ty = resolve_type_with_lifetimes(
                                    value,
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
                                assoc_bindings.push((member[0].clone(), val_ty));
                            }
                            ast::DynBindingKind::Type { .. }
                            | ast::DynBindingKind::Constant { .. }
                            | ast::DynBindingKind::Method { .. } => {
                                return Err(diag::unsupported(
                                    "unsupported associated binding in `dyn` type",
                                    binding.span,
                                ));
                            }
                        },
                        ast::GenericArg::Lifetime(_) => {}
                        ast::GenericArg::Const(_) => {
                            return Err(diag::unsupported(
                                "const generic argument in `dyn` type",
                                t.span,
                            ));
                        }
                    }
                }
                for binding in bindings {
                    match &binding.kind {
                        ast::DynBindingKind::Type { member, value } if member.len() == 1 => {
                            let val_ty = resolve_type_with_lifetimes(
                                value,
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
                            assoc_bindings.push((member[0].clone(), val_ty));
                        }
                        ast::DynBindingKind::Type { .. }
                        | ast::DynBindingKind::Constant { .. }
                        | ast::DynBindingKind::Method { .. } => {
                            return Err(diag::unsupported(
                                "unsupported binding in `dyn` type",
                                binding.span,
                            ));
                        }
                    }
                }
                assoc_bindings.sort_by(|a, b| a.0.cmp(&b.0));
                Ok(types.intern(TyData::Dyn {
                    trait_ref: tr,
                    trait_args,
                    assoc_bindings,
                }))
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
            let owner_args = (0..generics.len() as u32)
                .map(|i| types.intern(TyData::Param(i)))
                .collect();
            Ok(types.intern(TyData::ImplCallable {
                owner,
                owner_args,
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

fn eval_const_u32(
    expr: &ast::Expr,
    from: ModIdx,
    modules: &[ModuleScope],
    mod_by_path: &HashMap<String, ModIdx>,
) -> Option<u32> {
    if let ast::ExprKind::Literal(ast::Literal::Integer(text)) = &expr.kind {
        crate::literal::parse_int(text).and_then(|(v, suffix)| match suffix {
            None | Some(IntTy::Usize) => u32::try_from(v).ok(),
            Some(_) => None,
        })
    } else if let ast::ExprKind::Group(inner) = &expr.kind {
        eval_const_u32(inner, from, modules, mod_by_path)
    } else if let ast::ExprKind::Name(name) = &expr.kind {
        modules[from.0 as usize].const_u32.get(name).copied()
    } else if let ast::ExprKind::Member {
        object,
        member: ast::MemberName::Name(name),
    } = &expr.kind
    {
        let mut segs = Vec::new();
        let mut cur = object.as_ref();
        loop {
            if let ast::ExprKind::Name(id) = &cur.kind {
                segs.push(id.clone());
                break;
            } else if let ast::ExprKind::Member {
                object: o,
                member: ast::MemberName::Name(n),
            } = &cur.kind
            {
                segs.push(n.clone());
                cur = o;
            } else {
                return None;
            }
        }
        segs.reverse();
        let target_mod = if segs.len() == 1 {
            modules[from.0 as usize].imports.get(&segs[0]).copied()
        } else {
            mod_by_path.get(&segs.join(".")).copied()
        }?;
        if target_mod != from && !modules[target_mod.0 as usize].publics.contains(name) {
            return None;
        }
        modules[target_mod.0 as usize].const_u32.get(name).copied()
    } else if let ast::ExprKind::Binary { op, left, right } = &expr.kind {
        let l = eval_const_u32(left, from, modules, mod_by_path)?;
        let r = eval_const_u32(right, from, modules, mod_by_path)?;
        if *op == ast::BinaryOp::Add {
            l.checked_add(r)
        } else if *op == ast::BinaryOp::Subtract {
            l.checked_sub(r)
        } else if *op == ast::BinaryOp::Multiply {
            l.checked_mul(r)
        } else if *op == ast::BinaryOp::Divide {
            l.checked_div(r)
        } else if *op == ast::BinaryOp::Remainder {
            l.checked_rem(r)
        } else {
            None
        }
    } else {
        None
    }
}
