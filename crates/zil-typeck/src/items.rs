//! resolve + collect: the item table of a program and the semantic form of
//! every declaration.
//!
//! Declarations become templates over their own type parameters
//! (`TyData::Param`). Field identity is "declaration + variant + ordinal";
//! names are kept for diagnostics and source resolution only.

use crate::diag;
use crate::thir::FnRef;
use crate::ty::{AdtRef, Ty, TyData, Types};
use std::collections::HashMap;
use zil_base::{Diagnostic, Span};
use zil_ir::body::{AdtKind, FloatTy, IntTy};
use zil_syntax::ast;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemRef {
    Fn(FnRef),
    Adt(AdtRef),
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
    pub ast: &'a ast::Function,
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
    pub names: HashMap<String, ItemRef>,
}

impl<'a> Items<'a> {
    pub fn adt(&self, adt: AdtRef) -> &AdtDecl {
        &self.adts[adt.0 as usize]
    }
    pub fn func(&self, f: FnRef) -> &FnDecl<'a> {
        &self.fns[f.0 as usize]
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
        names: HashMap::new(),
    };
    // Pass 1: names.
    let mut adt_asts: Vec<AdtAst<'a>> = Vec::new();
    let mut fn_asts: Vec<&ast::Function> = Vec::new();
    for item in &module.items {
        let (name, r) = match &item.kind {
            ast::ItemKind::Function(f) => {
                fn_asts.push(f);
                (&f.name, ItemRef::Fn(FnRef(fn_asts.len() as u32 - 1)))
            }
            ast::ItemKind::Aggregate(a) => {
                adt_asts.push(AdtAst::Aggregate(a, item.span));
                (&a.name, ItemRef::Adt(AdtRef(adt_asts.len() as u32 - 1)))
            }
            ast::ItemKind::Enum(e) => {
                adt_asts.push(AdtAst::Enum(e, item.span));
                (&e.name, ItemRef::Adt(AdtRef(adt_asts.len() as u32 - 1)))
            }
            ast::ItemKind::Import(_) => {
                diags.push(diag::unsupported("imports", item.span));
                continue;
            }
            ast::ItemKind::Trait(_) => {
                diags.push(diag::unsupported("trait declarations", item.span));
                continue;
            }
            ast::ItemKind::Impl(_) => {
                diags.push(diag::unsupported("impl blocks", item.span));
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
            ast::ItemKind::Extern { .. } => {
                diags.push(diag::unsupported("foreign declarations", item.span));
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
    // Pass 2: declarations. ADT ids are reserved in pass 1 so that fields
    // may refer to any declaration, including recursively.
    for item in adt_asts {
        let decl = collect_adt(item, &items.names, types, diags);
        items.adts.push(decl);
    }
    for f in fn_asts {
        let decl = collect_fn(f, &items.names, types, diags);
        items.fns.push(decl);
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

fn collect_adt(
    item: AdtAst<'_>,
    names: &HashMap<String, ItemRef>,
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
    let mut resolve = |t: &ast::TypeRef, diags: &mut Vec<Diagnostic>| {
        resolve_type(t, &scope, names, types).unwrap_or_else(|d| {
            diags.push(d);
            types.error()
        })
    };
    let fields_of = |fields: &[ast::Field],
                     diags: &mut Vec<Diagnostic>,
                     resolve: &mut dyn FnMut(&ast::TypeRef, &mut Vec<Diagnostic>) -> Ty|
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
            if f.manages {
                diags.push(diag::unsupported("managed fields", f.span));
            }
            out.push(FieldDecl {
                name: f.name.clone(),
                ty: resolve(&f.ty, diags),
            });
        }
        out
    };
    let variants = match item {
        AdtAst::Aggregate(a, _) => vec![VariantDecl {
            name: a.name.clone(),
            shape: VariantShape::Record,
            fields: fields_of(&a.fields, diags, &mut resolve),
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
                                diags.push(diag::unsupported("managed fields", f.span));
                            }
                            fs.push(FieldDecl {
                                name: i.to_string(),
                                ty: resolve(&f.ty, diags),
                            });
                        }
                        (VariantShape::Tuple, fs)
                    }
                    ast::VariantFields::Record(fields) => {
                        (VariantShape::Record, fields_of(fields, diags, &mut resolve))
                    }
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
    if kind != AdtKind::Record && kind != AdtKind::Enum {
        diags.push(diag::unsupported("package and resource declarations", span));
    }
    AdtDecl {
        name: name.clone(),
        kind,
        generics: scope,
        variants,
    }
}

fn collect_fn<'a>(
    f: &'a ast::Function,
    names: &HashMap<String, ItemRef>,
    types: &mut Types,
    diags: &mut Vec<Diagnostic>,
) -> FnDecl<'a> {
    let generics = type_generics(&f.generics, diags);
    let scope: Vec<String> = generics.iter().map(|g| g.name.clone()).collect();
    if !f.where_clause.is_empty() {
        diags.push(diag::unsupported("where clauses", f.span));
    }
    if !f.contract.is_empty() {
        diags.push(diag::unsupported("function contracts", f.span));
    }
    let mut params = Vec::new();
    for p in &f.params {
        if p.give {
            diags.push(diag::unsupported("give parameters", p.span));
        }
        let ty = resolve_type(&p.ty, &scope, names, types).unwrap_or_else(|d| {
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
        Some(t) => resolve_type(t, &scope, names, types).unwrap_or_else(|d| {
            diags.push(d);
            types.error()
        }),
        None => types.unit(),
    };
    FnDecl {
        name: f.name.clone(),
        generics,
        params,
        result,
        ast: f,
    }
}

/// Resolves a written type in the scope of `generics`.
#[allow(clippy::result_large_err)] // `Diagnostic` is the workspace-wide error type.
pub fn resolve_type(
    t: &ast::TypeRef,
    generics: &[String],
    names: &HashMap<String, ItemRef>,
    types: &mut Types,
) -> Result<Ty, Diagnostic> {
    match &t.kind {
        ast::TypeKind::Named { path, args } => {
            let [name] = path.as_slice() else {
                return Err(diag::unsupported("qualified type paths", t.span));
            };
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
            match names.get(name) {
                Some(ItemRef::Adt(adt)) => {
                    let mut tys = Vec::new();
                    for a in args {
                        match a {
                            ast::GenericArg::Type(t) => {
                                tys.push(resolve_type(t, generics, names, types)?)
                            }
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
                    Ok(types.intern(TyData::Adt {
                        adt: *adt,
                        args: tys,
                    }))
                }
                Some(ItemRef::Fn(_)) | None => Err(diag::error(
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
                tys.push(resolve_type(i, generics, names, types)?);
            }
            Ok(types.intern(TyData::Tuple(tys)))
        }
        ast::TypeKind::Group(inner) => resolve_type(inner, generics, names, types),
        ast::TypeKind::Array { element, length } => {
            let element = resolve_type(element, generics, names, types)?;
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
        ast::TypeKind::Reference { .. } => Err(diag::unsupported("reference types", t.span)),
        ast::TypeKind::Function { .. } => Err(diag::unsupported("function types", t.span)),
        ast::TypeKind::ForeignFunction { .. } => {
            Err(diag::unsupported("foreign function types", t.span))
        }
        ast::TypeKind::Optional(_) => Err(diag::unsupported("optional types", t.span)),
        ast::TypeKind::Error { .. } => Err(diag::unsupported("error union types", t.span)),
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
