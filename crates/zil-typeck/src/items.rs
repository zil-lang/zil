//! resolve + collect: the item table of a program and the semantic form of
//! every declaration.
//!
//! Declarations become templates over their own type parameters
//! (`TyData::Param`). Field identity is "declaration + variant + ordinal";
//! names are kept for diagnostics and source resolution only.

use crate::diag;
use crate::thir::{FnRef, ForeignRef};
use crate::ty::{AdtRef, OpaqueKind, Ty, TyData, Types};
use std::collections::HashMap;
use zil_base::{Diagnostic, Span};
use zil_ir::body::{AdtKind, FloatTy, IntTy};
use zil_syntax::ast;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemRef {
    Fn(FnRef),
    Adt(AdtRef),
    Foreign(ForeignRef),
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
}

#[derive(Clone, Debug)]
pub struct ParamInfo {
    pub name: String,
    pub ty: Ty,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct FnDecl<'a> {
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

pub struct Items<'a> {
    pub fns: Vec<FnDecl<'a>>,
    pub adts: Vec<AdtDecl>,
    pub foreigns: Vec<ForeignDeclInfo>,
    pub adt_kinds: Vec<AdtKind>,
    pub names: HashMap<String, ItemRef>,
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

/// Builds the item table of one module and collects every declaration.
pub fn collect<'a>(
    module: &'a ast::Module,
    types: &mut Types,
    diags: &mut Vec<Diagnostic>,
) -> Items<'a> {
    let mut items = Items {
        fns: Vec::new(),
        adts: Vec::new(),
        foreigns: Vec::new(),
        adt_kinds: Vec::new(),
        names: HashMap::new(),
    };
    // Pass 1: names and ADT kinds.
    let mut adt_asts: Vec<AdtAst<'a>> = Vec::new();
    let mut fn_asts: Vec<&ast::Function> = Vec::new();
    let mut foreign_asts: Vec<(&str, &ast::Function, Span)> = Vec::new();
    let mut impl_asts: Vec<(&ast::ImplBlock, Span)> = Vec::new();
    for item in &module.items {
        let (name, r) = match &item.kind {
            ast::ItemKind::Function(f) => {
                fn_asts.push(f);
                (&f.name, ItemRef::Fn(FnRef(fn_asts.len() as u32 - 1)))
            }
            ast::ItemKind::Aggregate(a) => {
                let kind = match a.kind {
                    ast::AggregateKind::Struct => AdtKind::Record,
                    ast::AggregateKind::Package => AdtKind::Package,
                    ast::AggregateKind::Resource => AdtKind::Resource,
                };
                items.adt_kinds.push(kind);
                adt_asts.push(AdtAst::Aggregate(a, item.span));
                (&a.name, ItemRef::Adt(AdtRef(adt_asts.len() as u32 - 1)))
            }
            ast::ItemKind::Enum(e) => {
                items.adt_kinds.push(AdtKind::Enum);
                adt_asts.push(AdtAst::Enum(e, item.span));
                (&e.name, ItemRef::Adt(AdtRef(adt_asts.len() as u32 - 1)))
            }
            ast::ItemKind::Extern { abi, function } => {
                foreign_asts.push((abi.as_str(), function, item.span));
                (
                    &function.name,
                    ItemRef::Foreign(ForeignRef(foreign_asts.len() as u32 - 1)),
                )
            }
            ast::ItemKind::Impl(imp) => {
                impl_asts.push((imp, item.span));
                continue;
            }
            ast::ItemKind::Import(_) => {
                diags.push(diag::unsupported("imports", item.span));
                continue;
            }
            ast::ItemKind::Trait(_) => {
                diags.push(diag::unsupported("trait declarations", item.span));
                continue;
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
        if items.names.insert(name.clone(), r).is_some() {
            diags.push(diag::error(
                diag::NAME_DUPLICATE,
                item.span,
                format!("`{name}` is declared more than once"),
            ));
        }
    }
    // Pass 2: declarations. ADT ids and kinds are fixed in pass 1 so that
    // fields and signatures may refer to any declaration, including bare
    // resource names resolving to Desc<R>.
    for item in adt_asts {
        let decl = collect_adt(item, &items.names, &items.adt_kinds, types, diags);
        items.adts.push(decl);
    }
    for f in fn_asts {
        let decl = collect_fn(f, &items.names, &items.adt_kinds, types, diags);
        items.fns.push(decl);
    }
    for (abi, f, span) in foreign_asts {
        let decl = collect_foreign(abi, f, span, &items.names, &items.adt_kinds, types, diags);
        items.foreigns.push(decl);
    }
    for (imp, span) in impl_asts {
        collect_impl(imp, span, &mut items, types, diags);
    }
    items
}

fn type_generics(generics: &[ast::GenericParam], diags: &mut Vec<Diagnostic>) -> Vec<GenericInfo> {
    let mut out = Vec::new();
    for g in generics {
        match &g.kind {
            ast::GenericParamKind::Type(bounds) => {
                let mut copy = false;
                for b in bounds {
                    if let ast::TypeKind::Named { path, args } = &b.kind
                        && args.is_empty()
                        && path == &["Copy"]
                    {
                        copy = true;
                    } else {
                        diags.push(diag::unsupported("trait bounds other than Copy", b.span));
                    }
                }
                out.push(GenericInfo {
                    name: g.name.clone(),
                    copy,
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

#[derive(Clone, Copy)]
enum AdtAst<'a> {
    Aggregate(&'a ast::Aggregate, Span),
    Enum(&'a ast::EnumDecl, Span),
}

/// Promotes `Desc(inner)` or `Managed(inner)` to `Managed(inner)`.
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
        | TyData::Ref { .. }
        | TyData::Opaque(_)
        | TyData::ErrorUnion { .. }
        | TyData::Param(_)
        | TyData::Var(_) => None,
    }
}

/// Promotes a bare resource / `Store<T>` / `ByteBuffer` (or `E!R`) into its
/// owning `Managed` carrier for `give` parameters and `result(create|give)`.
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
    item: AdtAst<'_>,
    names: &HashMap<String, ItemRef>,
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
    let generic_info = type_generics(generics, diags);
    let scope: Vec<String> = generic_info.iter().map(|g| g.name.clone()).collect();
    let mut resolve = |t: &ast::TypeRef, diags: &mut Vec<Diagnostic>, types: &mut Types| {
        resolve_type(t, &scope, names, adt_kinds, None, types).unwrap_or_else(|d| {
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
                    // Non-managed fields cannot hold bare ByteBuffer or Store descriptors.
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
        name: name.clone(),
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

fn collect_fn<'a>(
    f: &'a ast::Function,
    names: &HashMap<String, ItemRef>,
    adt_kinds: &[AdtKind],
    types: &mut Types,
    diags: &mut Vec<Diagnostic>,
) -> FnDecl<'a> {
    let generics = type_generics(&f.generics, diags);
    let scope: Vec<String> = generics.iter().map(|g| g.name.clone()).collect();
    if !f.where_clause.is_empty() {
        diags.push(diag::unsupported("where clauses", f.span));
    }
    let result_mode = parse_result_mode(&f.contract, diags);
    let mut params = Vec::new();
    for p in &f.params {
        let raw_ty =
            resolve_type(&p.ty, &scope, names, adt_kinds, None, types).unwrap_or_else(|d| {
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
        Some(t) => resolve_type(t, &scope, names, adt_kinds, None, types).unwrap_or_else(|d| {
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
        name: f.name.clone(),
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

fn collect_foreign(
    abi: &str,
    f: &ast::Function,
    span: Span,
    names: &HashMap<String, ItemRef>,
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
        let ty = resolve_type(&p.ty, &[], names, adt_kinds, None, types).unwrap_or_else(|d| {
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
        Some(t) => resolve_type(t, &[], names, adt_kinds, None, types).unwrap_or_else(|d| {
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
        diags.push(diag::unsupported("impl blocks", span));
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
    let target_adt = match items.names.get(&path[0]) {
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
        Some(ItemRef::Adt(_) | ItemRef::Fn(_) | ItemRef::Foreign(_)) | None => {
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
    let p_ty = resolve_type(
        &p.ty,
        &[],
        &items.names,
        &items.adt_kinds,
        Some(self_desc),
        types,
    )
    .unwrap_or_else(|d| {
        diags.push(d);
        types.error()
    });
    let ret_ty = match &f.return_type {
        Some(t) => resolve_type(
            t,
            &[],
            &items.names,
            &items.adt_kinds,
            Some(self_desc),
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

/// Resolves a written type in the scope of `generics`.
#[allow(clippy::result_large_err)] // `Diagnostic` is the workspace-wide error type.
pub fn resolve_type(
    t: &ast::TypeRef,
    generics: &[String],
    names: &HashMap<String, ItemRef>,
    adt_kinds: &[AdtKind],
    self_ty: Option<Ty>,
    types: &mut Types,
) -> Result<Ty, Diagnostic> {
    match &t.kind {
        ast::TypeKind::Named { path, args } => {
            let [name] = path.as_slice() else {
                return Err(diag::unsupported("qualified type paths", t.span));
            };
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
                let elem = resolve_type(elem, generics, names, adt_kinds, self_ty, types)?;
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
                let resolved = resolve_type(inner_ast, generics, names, adt_kinds, self_ty, types)?;
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
            match names.get(name) {
                Some(ItemRef::Adt(adt)) => {
                    let mut tys = Vec::new();
                    for a in args {
                        match a {
                            ast::GenericArg::Type(t) => tys
                                .push(resolve_type(t, generics, names, adt_kinds, self_ty, types)?),
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
                    let adt_ty = types.intern(TyData::Adt {
                        adt: *adt,
                        args: tys,
                    });
                    // Bare resource names denote their descriptor `Desc<R>`.
                    if adt_kinds[adt.0 as usize] == AdtKind::Resource {
                        Ok(types.intern(TyData::Desc(adt_ty)))
                    } else {
                        Ok(adt_ty)
                    }
                }
                Some(ItemRef::Fn(_) | ItemRef::Foreign(_)) | None => Err(diag::error(
                    diag::NAME_UNRESOLVED,
                    t.span,
                    format!("unknown type `{name}`"),
                )),
            }
        }
        ast::TypeKind::Tuple(items) => {
            if items.is_empty() {
                return Ok(types.unit());
            }
            let mut tys = Vec::new();
            for i in items {
                tys.push(resolve_type(i, generics, names, adt_kinds, self_ty, types)?);
            }
            Ok(types.intern(TyData::Tuple(tys)))
        }
        ast::TypeKind::Group(inner) => {
            resolve_type(inner, generics, names, adt_kinds, self_ty, types)
        }
        ast::TypeKind::Array { element, length } => {
            let element = resolve_type(element, generics, names, adt_kinds, self_ty, types)?;
            let Some(length) = length else {
                return Err(diag::unsupported("slices", t.span));
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
            let pointee = resolve_type(inner, generics, names, adt_kinds, self_ty, types)?;
            Ok(types.intern(TyData::Ref {
                mutable: *mutable,
                pointee,
            }))
        }
        ast::TypeKind::Error { error, result } => {
            let err = resolve_type(error, generics, names, adt_kinds, self_ty, types)?;
            let ok = resolve_type(result, generics, names, adt_kinds, self_ty, types)?;
            Ok(types.intern(TyData::ErrorUnion { ok, err }))
        }
        ast::TypeKind::Function { .. } => Err(diag::unsupported("function types", t.span)),
        ast::TypeKind::ForeignFunction { .. } => {
            Err(diag::unsupported("foreign function types", t.span))
        }
        ast::TypeKind::Optional(_) => Err(diag::unsupported("optional types", t.span)),
        ast::TypeKind::Union(_) => Err(diag::unsupported("anonymous sum types", t.span)),
        ast::TypeKind::Dyn { .. } => Err(diag::unsupported("dyn types", t.span)),
        ast::TypeKind::ForLifetime { .. } => {
            Err(diag::unsupported("higher-ranked lifetimes", t.span))
        }
        ast::TypeKind::Impl(_) => Err(diag::unsupported("impl types", t.span)),
        ast::TypeKind::MetaEval(_) | ast::TypeKind::MetaSplice(_) | ast::TypeKind::MetaBlock(_) => {
            Err(diag::unsupported("compile-time types", t.span))
        }
    }
}
