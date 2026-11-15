//! Universal checking of the actual, unexpanded CT declarations. This input is
//! syntax, not a producer's selected instance or a proposed executable package.
//! The private terms below are interpreter state and are never published.
use crate::CtVerificationMeter;
use std::collections::{BTreeMap, BTreeSet};
use zil_base::{CompileResult, Diagnostic, Limits, ModuleId, Phase, Span, TargetProfile};
use zil_ir::ct_artifact::{CtScalarTy as Scalar, CtSyntaxCategory as Category};
use zil_syntax::ast as s;

mod builtins;
mod calls;
mod declarations;
mod expressions;
mod memory;
mod statements;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CtOriginalImportTarget {
    Module(ModuleId),
    Meta,
    Syntax,
}
pub struct CtOriginalImport<'a> {
    pub alias: &'a str,
    pub target: CtOriginalImportTarget,
}
pub struct CtOriginalModule<'a> {
    pub syntax: &'a s::Module,
    pub imports: &'a [CtOriginalImport<'a>],
}

/// Check every CT-only declaration in its own original mixed formal context,
/// including declarations without an actual call and every original branch.
/// Generated trees are the complete immutable event catalogue, including trees
/// whose output was subsequently consumed. No entry or query is manufactured.
pub fn verify_ct_original_definitions(
    modules: &[CtOriginalModule<'_>],
    generated: &[&s::Module],
    target: &TargetProfile,
    limits: &Limits,
    meter: &mut CtVerificationMeter,
) -> CompileResult<()> {
    Checker::new(modules, generated, target, limits, meter)
        .and_then(|mut checker| checker.check())
        .map_err(|e| vec![e])
}

type R<T> = Result<T, Diagnostic>;
type Id = usize;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Binder {
    owner: Id,
    ordinal: usize,
}
#[derive(Clone, Debug, PartialEq, Eq)]
enum Ty {
    Scalar(Scalar),
    Meta(Meta),
    Parameter(Binder),
    Tuple(Vec<Ty>),
    Array(Box<Ty>, Constant),
    List(Box<Ty>),
    Sum(Vec<Ty>),
    Nominal(Id, Vec<Actual>),
    // Type-valued source constructors can describe runtime types without
    // admitting those types as values in the pure CT evaluator.
    Runtime(&'static str, Vec<Actual>),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Meta {
    Type,
    Syntax(Category),
    Ident,
    TokenTree,
    TokenCursor,
    Text,
    Shape,
    Field,
    Impl,
    Layout,
}
#[derive(Clone, Debug, PartialEq, Eq)]
enum Actual {
    Type(Ty),
    Const(Constant),
    Life(Binder),
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Constant {
    carrier: Scalar,
    term: Term,
}
#[derive(Clone, Debug, PartialEq, Eq)]
enum Term {
    Parameter(Binder),
    Integer(i128),
    Bool(bool),
    Literal(String),
    Unary(s::UnaryOp, Box<Constant>),
    Binary(s::BinaryOp, Box<Constant>, Box<Constant>),
    Convert(Box<Constant>),
}
#[derive(Clone)]
struct Value {
    ty: Ty,
    constant: Option<Constant>,
    represented: Option<Ty>,
}
impl Value {
    fn typed(ty: Ty) -> Self {
        Self {
            ty,
            constant: None,
            represented: None,
        }
    }
    fn type_value(ty: Ty) -> Self {
        Self {
            ty: Ty::Meta(Meta::Type),
            constant: None,
            represented: Some(ty),
        }
    }
}
#[derive(Clone)]
struct Binding {
    name: String,
    value: Value,
    mutable: bool,
}
#[derive(Clone)]
struct Env {
    owner: Id,
    module: usize,
    generics: Vec<(String, Actual)>,
    values: Vec<Binding>,
    result: Ty,
    yielding: Option<Option<Ty>>,
    loops: usize,
}
impl Env {
    fn actual(&self, name: &str) -> Option<&Actual> {
        self.generics
            .iter()
            .rev()
            .find(|(n, _)| n == name)
            .map(|(_, value)| value)
    }
    fn value(&self, name: &str) -> Option<&Binding> {
        self.values.iter().rev().find(|row| row.name == name)
    }
}
#[derive(Clone, Copy)]
struct Declaration<'a> {
    item: &'a s::Item,
    module: usize,
    parent: Option<Id>,
}
struct Checker<'a, 'l, 'm> {
    modules: &'a [CtOriginalModule<'a>],
    declarations: Vec<Declaration<'a>>,
    by_name: BTreeMap<(usize, Option<Id>, &'a str), Vec<Id>>,
    ct_catalogue: BTreeMap<Id, bool>,
    module_ids: BTreeMap<ModuleId, usize>,
    limits: &'l Limits,
    target: &'l TargetProfile,
    meter: &'m mut CtVerificationMeter,
    checked: BTreeSet<Id>,
    checking: BTreeSet<Id>,
    nominal_active: BTreeSet<Id>,
    aliases: BTreeSet<Id>,
    constants: BTreeSet<Id>,
    nodes: usize,
    bytes: usize,
    spent_at_start: u64,
}
fn bad(span: Span, message: impl Into<String>) -> Diagnostic {
    Diagnostic::error(Phase::Ct, "E-CT-ORIGINAL", Some(span), message)
}
fn unit() -> Ty {
    Ty::Scalar(Scalar::Unit)
}
fn bool_ty() -> Ty {
    Ty::Scalar(Scalar::Bool)
}
fn scalar(name: &str) -> Option<Scalar> {
    Some(match name {
        "void" => Scalar::Unit,
        "bool" => Scalar::Bool,
        "i8" => Scalar::I8,
        "i16" => Scalar::I16,
        "i32" => Scalar::I32,
        "i64" => Scalar::I64,
        "isize" => Scalar::Isize,
        "u8" => Scalar::U8,
        "u16" => Scalar::U16,
        "u32" => Scalar::U32,
        "u64" => Scalar::U64,
        "usize" => Scalar::Usize,
        "f32" => Scalar::F32,
        "f64" => Scalar::F64,
        _ => return None,
    })
}
fn integer(ty: &Ty) -> bool {
    matches!(ty, Ty::Scalar(s) if !matches!(s, Scalar::Unit|Scalar::Bool|Scalar::F32|Scalar::F64))
}
fn number(ty: &Ty) -> bool {
    matches!(ty, Ty::Scalar(s) if !matches!(s, Scalar::Unit|Scalar::Bool))
}
fn category(name: &str) -> Option<Category> {
    Some(match name {
        "Expr" => Category::Expr,
        "Type" => Category::Type,
        "StmtList" => Category::StmtList,
        "ItemList" => Category::ItemList,
        _ => return None,
    })
}
fn meta(name: &str) -> Option<Meta> {
    Some(match name {
        "Type" => Meta::Type,
        "Ident" => Meta::Ident,
        "TokenTree" => Meta::TokenTree,
        "TokenCursor" => Meta::TokenCursor,
        "Text" => Meta::Text,
        "ShapeInfo" => Meta::Shape,
        "FieldInfo" => Meta::Field,
        "ImplInfo" => Meta::Impl,
        "LayoutInfo" => Meta::Layout,
        _ => return None,
    })
}
fn name(item: &s::Item) -> Option<&str> {
    Some(match &item.kind {
        s::ItemKind::Function(f) | s::ItemKind::Extern { function: f, .. } => &f.name,
        s::ItemKind::Aggregate(a) => &a.name,
        s::ItemKind::Enum(e) => &e.name,
        s::ItemKind::Trait(t) => &t.name,
        s::ItemKind::TypeAlias(a) => &a.name,
        s::ItemKind::Const(c) => &c.name,
        _ => return None,
    })
}
fn formals(item: &s::Item) -> &[s::GenericParam] {
    match &item.kind {
        s::ItemKind::Function(f) | s::ItemKind::Extern { function: f, .. } => &f.generics,
        s::ItemKind::Aggregate(a) => &a.generics,
        s::ItemKind::Enum(e) => &e.generics,
        s::ItemKind::Trait(t) => &t.generics,
        s::ItemKind::Impl(i) => &i.generics,
        s::ItemKind::TypeAlias(a) => &a.generics,
        _ => &[],
    }
}

impl<'a, 'l, 'm> Checker<'a, 'l, 'm> {
    fn new(
        modules: &'a [CtOriginalModule<'a>],
        generated: &[&'a s::Module],
        target: &'l TargetProfile,
        limits: &'l Limits,
        meter: &'m mut CtVerificationMeter,
    ) -> R<Self> {
        if !matches!(target.pointer_width, 32 | 64) {
            return Err(bad(
                Span::default(),
                "CT original checking requires the actual supported target width",
            ));
        }
        let spent_at_start = meter.spent();
        let mut checker = Self {
            modules,
            declarations: vec![],
            by_name: BTreeMap::new(),
            ct_catalogue: BTreeMap::new(),
            module_ids: BTreeMap::new(),
            limits,
            target,
            meter,
            checked: BTreeSet::new(),
            checking: BTreeSet::new(),
            nominal_active: BTreeSet::new(),
            aliases: BTreeSet::new(),
            constants: BTreeSet::new(),
            nodes: 0,
            bytes: 0,
            spent_at_start,
        };
        for (index, module) in modules.iter().enumerate() {
            checker.step(module.syntax.span, 0)?;
            if checker.module_ids.insert(module.syntax.id, index).is_some() {
                return Err(bad(
                    module.syntax.span,
                    "duplicate complete CT source module",
                ));
            }
        }
        for (index, module) in modules.iter().enumerate() {
            checker.validate_imports(index)?;
            for item in &module.syntax.items {
                checker.collect(item, index, None, 0)?;
            }
        }
        for tree in generated {
            checker.step(tree.span, 0)?;
            let module = *checker
                .module_ids
                .get(&tree.id)
                .ok_or_else(|| bad(tree.span, "generated CT tree has no actual source module"))?;
            for item in &tree.items {
                checker.collect(item, module, None, 0)?;
            }
        }
        Ok(checker)
    }
    fn work(&mut self, n: usize, span: Span) -> R<()> {
        if self
            .meter
            .spent()
            .saturating_sub(self.spent_at_start)
            .saturating_add(n as u64)
            > self.limits.max_analysis_steps as u64
        {
            return Err(Diagnostic::limit(
                Phase::Ct,
                "CT original verification work exhausted",
                Some(span),
            ));
        }
        self.meter.charge(n, Some(span)).map_err(|errors| {
            errors.into_iter().next().unwrap_or_else(|| {
                Diagnostic::limit(
                    Phase::Ct,
                    "CT original verification meter rejected without a diagnostic",
                    Some(span),
                )
            })
        })
    }
    fn alloc(&mut self, n: usize, span: Span) -> R<()> {
        self.work(n.saturating_add(1), span)?;
        self.bytes = self
            .bytes
            .checked_add(n)
            .ok_or_else(|| bad(span, "CT original allocation overflow"))?;
        if self.bytes > self.limits.max_source_bytes {
            return Err(Diagnostic::limit(
                Phase::Ct,
                "CT original temporary storage exhausted",
                Some(span),
            ));
        }
        Ok(())
    }
    fn step(&mut self, span: Span, depth: usize) -> R<()> {
        self.work(1, span)?;
        self.nodes = self.nodes.saturating_add(1);
        if depth >= self.limits.max_call_depth.min(128) || self.nodes > self.limits.max_nodes {
            return Err(Diagnostic::limit(
                Phase::Ct,
                "CT original syntax/type budget exhausted",
                Some(span),
            ));
        }
        Ok(())
    }
    fn same(&mut self, a: &Ty, b: &Ty, span: Span) -> R<()> {
        self.type_work(a, span, 0)?;
        self.type_work(b, span, 0)?;
        if a != b {
            return Err(bad(
                span,
                "original CT types differ in the complete declaration context",
            ));
        }
        Ok(())
    }
    fn type_work(&mut self, ty: &Ty, span: Span, depth: usize) -> R<()> {
        self.step(span, depth)?;
        match ty {
            Ty::Tuple(ts) | Ty::Sum(ts) => {
                for ty in ts {
                    self.type_work(ty, span, depth + 1)?;
                }
            }
            Ty::Array(t, n) => {
                self.type_work(t, span, depth + 1)?;
                self.constant_work(n, span, depth + 1)?;
            }
            Ty::List(t) => self.type_work(t, span, depth + 1)?,
            Ty::Nominal(_, args) | Ty::Runtime(_, args) => {
                for arg in args {
                    self.work(1, span)?;
                    if let Actual::Type(ty) = arg {
                        self.type_work(ty, span, depth + 1)?;
                    } else if let Actual::Const(value) = arg {
                        self.constant_work(value, span, depth + 1)?;
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
    fn constant_work(&mut self, value: &Constant, span: Span, depth: usize) -> R<()> {
        self.step(span, depth)?;
        match &value.term {
            Term::Literal(text) => self.work(text.len(), span)?,
            Term::Unary(_, inner) | Term::Convert(inner) => {
                self.constant_work(inner, span, depth + 1)?
            }
            Term::Binary(_, a, b) => {
                self.constant_work(a, span, depth + 1)?;
                self.constant_work(b, span, depth + 1)?;
            }
            _ => {}
        }
        Ok(())
    }
    fn environment_copy(&mut self, env: &Env, span: Span) -> R<Env> {
        self.copy_environment(env, span)
    }
    fn collect(
        &mut self,
        item: &'a s::Item,
        module: usize,
        parent: Option<Id>,
        depth: usize,
    ) -> R<()> {
        self.step(item.span, depth)?;
        self.alloc(std::mem::size_of::<Declaration>(), item.span)?;
        let id = self.declarations.len();
        self.declarations.push(Declaration {
            item,
            module,
            parent,
        });
        if let Some(name) = name(item) {
            self.alloc(name.len() + std::mem::size_of::<Id>() * 4, item.span)?;
            self.by_name
                .entry((module, parent, name))
                .or_default()
                .push(id);
        }
        let children = match &item.kind {
            s::ItemKind::Aggregate(a) => &a.methods,
            s::ItemKind::Trait(t) => &t.members,
            s::ItemKind::Impl(i) => &i.members,
            _ => return Ok(()),
        };
        for child in children {
            self.collect(child, module, Some(id), depth + 1)?;
        }
        Ok(())
    }
    fn validate_imports(&mut self, index: usize) -> R<()> {
        let module = &self.modules[index];
        let mut aliases = BTreeSet::new();
        for import in module.imports {
            self.work(import.alias.len().saturating_add(1), module.syntax.span)?;
            if !aliases.insert(import.alias) {
                return Err(bad(module.syntax.span, "duplicate actual CT import alias"));
            }
            if let CtOriginalImportTarget::Module(id) = import.target {
                if !self.module_ids.contains_key(&id) {
                    return Err(bad(
                        module.syntax.span,
                        "CT import leaves the complete module graph",
                    ));
                }
            }
        }
        let mut count = 0;
        for item in &module.syntax.items {
            if let s::ItemKind::Import(import) = &item.kind {
                self.step(item.span, 0)?;
                if import.syntax {
                    continue;
                }
                count += 1;
                let alias = import
                    .alias
                    .as_deref()
                    .or_else(|| import.path.last().map(String::as_str))
                    .ok_or_else(|| bad(item.span, "empty CT import"))?;
                let edge = module
                    .imports
                    .iter()
                    .find(|row| row.alias == alias)
                    .ok_or_else(|| bad(item.span, "CT import has no actual frozen edge"))?;
                let builtin = if import.path.as_slice() == ["std", "meta"] {
                    Some(CtOriginalImportTarget::Meta)
                } else if import.path.as_slice() == ["std", "syntax"] {
                    Some(CtOriginalImportTarget::Syntax)
                } else {
                    None
                };
                if builtin.is_some_and(|expected| edge.target != expected)
                    || builtin.is_none()
                        && !matches!(edge.target, CtOriginalImportTarget::Module(_))
                {
                    return Err(bad(
                        item.span,
                        "CT builtin import differs from its actual source binding",
                    ));
                }
            }
        }
        if count != module.imports.len() {
            return Err(bad(
                module.syntax.span,
                "CT frozen imports add or omit an original edge",
            ));
        }
        Ok(())
    }
    fn resolve(&mut self, env: &Env, path: &[String], span: Span) -> R<Id> {
        self.work(
            path.iter().map(String::len).sum::<usize>().saturating_add(
                self.by_name
                    .len()
                    .checked_ilog2()
                    .map_or(1, |n| n as usize + 2),
            ),
            span,
        )?;
        if path.is_empty() {
            return Err(bad(span, "empty original CT name"));
        }
        let mut module = env.module;
        let mut start = 0;
        if path.len() > 1 {
            if let Some(import) = self.modules[module]
                .imports
                .iter()
                .find(|row| row.alias == path[0])
            {
                let CtOriginalImportTarget::Module(id) = import.target else {
                    return Err(bad(
                        span,
                        "CT standard module is not an ordinary declaration",
                    ));
                };
                module = *self
                    .module_ids
                    .get(&id)
                    .ok_or_else(|| bad(span, "CT import module is absent"))?;
                start = 1;
            }
        }
        let mut parent = None;
        if start == 0 {
            let host = self.declarations[env.owner].parent;
            if let Some(host) = host {
                if path.len() == 1
                    && self
                        .by_name
                        .contains_key(&(module, Some(host), path[0].as_str()))
                {
                    parent = Some(host);
                }
            }
        }
        let mut selected = None;
        for segment in &path[start..] {
            let matches = self
                .by_name
                .get(&(module, parent, segment.as_str()))
                .ok_or_else(|| {
                    bad(
                        span,
                        "unknown original CT declaration in the complete source scope",
                    )
                })?;
            if matches.len() != 1 {
                return Err(bad(span, "ambiguous original CT declaration identity"));
            }
            let id = matches[0];
            let row = self.declarations[id];
            if module != env.module && !row.item.public {
                return Err(bad(
                    span,
                    "original CT name is private to its source module",
                ));
            }
            selected = Some(id);
            parent = Some(id);
        }
        selected.ok_or_else(|| bad(span, "CT path names only a module"))
    }
}
