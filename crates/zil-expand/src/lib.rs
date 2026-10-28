#![forbid(unsafe_code)]
//! Source-shaped AST to untrusted Core. No transformation grants safety facts.
//! Sequence bindings identify ordered evaluations. They do not decide a place's
//! Copy/Move/Borrow mode or extend a borrow; those decisions remain in zil-types.
mod enums;
mod foreign;
mod generics;
mod mapping;
mod modules;
mod receivers;
mod records;
mod resources;
mod static_traits;

use std::collections::{BTreeMap, BTreeSet};
use zil_base::{
    CompileResult, Diagnostic, ExpansionRole, Limits, NodeId, Phase, SCHEMA_VERSION, SourceMap,
    SourceMapId, SourceTarget, SourceUnit, Span,
};
use zil_ir::*;
use zil_syntax::ast as s;

/// These names are source primitives, never ordinary function values.
fn integer_name(name: &str) -> bool {
    matches!(name, "u8" | "u16" | "i32" | "i64")
}
fn foreign_constructor_name(name: &str) -> bool {
    matches!(name, "c_null" | "c_wstr_null" | "c_wstr" | "c_callback")
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
        "Store"
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
fn primitive_name(name: &str) -> bool {
    integer_name(name)
        || foreign_constructor_name(name)
        || byte_buffer_name(name)
        || owned_store_name(name)
}
fn primitive_name_error(name: &str, span: Span) -> Diagnostic {
    if owned_store_name(name) {
        Diagnostic::error(
            Phase::Expand,
            "E-OWNED-STORE-NAME",
            Some(span),
            "Store and bare owned storage primitives cannot be rebound",
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
        "u8, u16, i32 and i64 are reserved bare integer conversion names",
    )
}

#[derive(Clone, Debug)]
pub struct ExpandedModule {
    pub core: CoreModule,
    pub source_map: SourceMap,
    // Compiler-generated ordinary imports, never safety capabilities.
    generated_imports: Vec<CoreImport>,
    generated_exports: Vec<NodeId>,
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
    let specialized = generics::specialize(module, limits)?;
    expand_inner(source, &specialized, false)
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
    let specialized = generics::specialize(module, limits)?;
    let module = &specialized;
    let expanded = expand_inner(source, module, true)?;
    let exports = source_exports(module, &expanded.core);
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
/// public local trait are public through their public concrete implementation.
fn source_exports(module: &s::Module, core: &CoreModule) -> Vec<NodeId> {
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
        .map(|f| ((f.name.clone(), f.span), f.id))
        .collect();
    let mut result = Vec::new();
    let mut add = |f: &s::Function, prefix: Option<&str>| {
        let name = prefix.map_or_else(|| f.name.clone(), |owner| format!("{owner}.{}", f.name));
        if let Some(id) = functions.get(&(name, f.span)) {
            result.push(*id);
        }
    };
    for item in &module.items {
        match &item.kind {
            s::ItemKind::Function(f) if item.public => add(f, None),
            s::ItemKind::Aggregate(a) if public_nominals.contains(a.name.as_str()) => {
                for member in &a.methods {
                    if member.public
                        && let s::ItemKind::Function(f) = &member.kind
                    {
                        add(f, Some(&a.name));
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
                    .is_some_and(|name| public_traits.contains(name.as_str()));
                for member in &i.members {
                    if (if i.trait_type.is_some() {
                        public_trait
                    } else {
                        member.public
                    }) && let s::ItemKind::Function(f) = &member.kind
                    {
                        add(f, Some(&owner));
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
    let specialized = modules::specialize(sources, syntax, imports, limits)?;
    let declarations = specialized.iter().map(|(m, _)| m).collect::<Vec<_>>();
    let mut result = Vec::new();
    let mut remaining = limits.clone();
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
    for (source, (module, imports)) in sources.iter().zip(&specialized) {
        let expanded = expand_inner_project(
            source,
            module,
            true,
            sources,
            &declarations,
            &remaining,
            Some(&defaults),
        )?;
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
        let mut exports = source_exports(module, &expanded.core);
        exports.extend(expanded.generated_exports);
        let mut resolved_imports = imports.clone();
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
    Ok(result)
}

fn expand_inner(
    source: &SourceUnit,
    module: &s::Module,
    imports_resolved: bool,
) -> CompileResult<ExpandedModule> {
    expand_inner_project(
        source,
        module,
        imports_resolved,
        std::slice::from_ref(source),
        &[module],
        &Limits::default(),
        None,
    )
}

fn expand_inner_project(
    source: &SourceUnit,
    module: &s::Module,
    imports_resolved: bool,
    sources: &[SourceUnit],
    declarations: &[&s::Module],
    limits: &Limits,
    project_defaults: Option<&ProjectDefaults>,
) -> CompileResult<ExpandedModule> {
    let mut e = Expander {
        imports_resolved,
        module_id: module.id,
        callable_work: limits.max_analysis_steps,
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
        declarations: BTreeMap::new(),
        default_helpers: BTreeMap::new(),
        traits: BTreeMap::new(),
        static_traits: BTreeSet::new(),
        static_trait_impls: BTreeSet::new(),
        source_map: SourceMap::new(module.id),
        node_maps: BTreeMap::new(),
        markers: BTreeMap::new(),
        parent_map: None,
        lexical_receiver: None,
        self_type: None,
        return_type: None,
        drop_implementations: BTreeMap::new(),
    };
    // Declaration collection does not resolve traits, evaluate defaults, or
    // grant capabilities. It supplies only already-written local syntax.
    for item in declarations.iter().flat_map(|module| &module.items) {
        match &item.kind {
            s::ItemKind::Aggregate(a) => {
                e.declarations.entry(a.name.clone()).or_insert(a);
            }
            s::ItemKind::Trait(_) => {}
            _ => {}
        }
    }
    for item in &module.items {
        if let s::ItemKind::Trait(t) = &item.kind {
            e.traits.entry(t.name.clone()).or_insert(t);
        }
    }
    e.collect_resource_drops(module);
    e.collect_static_traits(module, limits);
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
    for item in &module.items {
        e.item(item, None, None, None);
    }
    if !e.diagnostics.is_empty() {
        return Err(e.diagnostics);
    }
    e.source_map.validate(sources, limits)?;
    let core = CoreModule {
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
    })
}
struct Expander<'a> {
    callable_work: usize,
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
    declarations: BTreeMap<String, &'a s::Aggregate>,
    default_helpers: BTreeMap<(String, String), String>,
    traits: BTreeMap<String, &'a s::TraitDecl>,
    static_traits: BTreeSet<String>,
    static_trait_impls: BTreeSet<Span>,
    source_map: SourceMap,
    node_maps: BTreeMap<NodeId, SourceMapId>,
    markers: BTreeMap<Span, SourceMapId>,
    parent_map: Option<SourceMapId>,
    lexical_receiver: Option<Span>,
    self_type: Option<Type>,
    return_type: Option<Type>,
    drop_implementations: BTreeMap<String, String>,
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
            s::ItemKind::Function(f) => self.function(f, prefix, host, default),
            s::ItemKind::Aggregate(a) => {
                for field in &a.fields {
                    self.unexecuted_attributes(&field.attributes, "field");
                }
                if !a.generics.is_empty() {
                    self.unsupported("generic aggregate", item.span)
                }
                let id = self.id();
                self.map_node(id, item.span, ExpansionRole::Source, Vec::new());
                self.aggregates.push(CoreAggregate {
                    teardown: self.drop_implementations.get(&a.name).cloned(),
                    id,
                    name: a.name.clone(),
                    kind: match a.kind {
                        s::AggregateKind::Struct => AggregateKind::Struct,
                        s::AggregateKind::Resource => AggregateKind::Resource,
                        s::AggregateKind::Package => AggregateKind::Package,
                    },
                    fields: a
                        .fields
                        .iter()
                        .map(|f| CoreField {
                            name: f.name.clone(),
                            ty: type_ref(&f.ty),
                            public: f.public,
                            manages: f.manages,
                            span: f.span,
                        })
                        .collect(),
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
                if i.trait_type.is_some() && !self.static_trait_impls.contains(&item.span) {
                    self.unsupported("trait implementation resolution", item.span)
                }
                let name = type_path(&i.target).unwrap_or_else(|| "$unsupported".into());
                let host = type_ref(&i.target);
                for m in &i.members {
                    let inherited = if let Some(trait_type) = &i.trait_type {
                        self.trait_receiver(trait_type, m)
                    } else {
                        i.receiver_default.clone()
                    };
                    self.item(m, Some(&name), Some(&host), inherited.as_ref())
                }
            }
            s::ItemKind::Import(_) if self.imports_resolved => {}
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
        self.unexecuted_attributes(&p.attributes, "parameter");
        CoreParameter {
            name: p.name.clone(),
            ty: self.type_ref(&p.ty),
            mode: mode(p),
            span: p.span,
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
            CoreExprKind::Member { object, member } => {
                let (object, read_object) = self.compound_place(*object, bindings)?;
                (
                    CoreExprKind::Member {
                        object: Box::new(object),
                        member: member.clone(),
                    },
                    CoreExprKind::Member {
                        object: Box::new(read_object),
                        member,
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
    fn stmt(&mut self, s: &s::Stmt) -> CoreStatement {
        let kind = match &s.kind {
            s::StmtKind::Binding {
                mutable,
                pattern,
                ty,
                value,
            } => {
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
                let ty = ty.as_ref().map(|t| self.type_ref(t));
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
                CoreStatementKind::ForRange {
                    name: name.clone(),
                    start: self.expr(left),
                    end: self.expr(right),
                    body: self.block(body),
                }
            }
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
    fn expr(&mut self, e: &s::Expr) -> CoreExpr {
        self.expr_expected(e, None)
    }
    fn expr_expected(&mut self, e: &s::Expr, expected: Option<&Type>) -> CoreExpr {
        let kind = match &e.kind {
            s::ExprKind::Literal(l) => match l {
                s::Literal::Integer(v) => CoreExprKind::Integer(v.clone()),
                s::Literal::Bool(v) => CoreExprKind::Bool(*v),
                s::Literal::String(v) => CoreExprKind::String(v.clone()),
                _ => CoreExprKind::Unsupported("floating/character literal typing".into()),
            },
            s::ExprKind::Name(n) => CoreExprKind::Name(n.clone()),
            s::ExprKind::ReceiverMember(n) => return self.receiver_member(n, e.span),
            s::ExprKind::Group(v) => return self.expr_expected(v, expected),
            s::ExprKind::Array(values) => {
                CoreExprKind::Array(values.iter().map(|v| self.expr(v)).collect())
            }
            s::ExprKind::Index { object, index } => CoreExprKind::Index {
                object: Box::new(self.expr(object)),
                index: Box::new(self.expr(index)),
            },
            s::ExprKind::Slice { object, start, end } => CoreExprKind::Slice {
                object: Box::new(self.expr(object)),
                start: start.as_ref().map(|value| Box::new(self.expr(value))),
                end: end.as_ref().map(|value| Box::new(self.expr(value))),
            },
            s::ExprKind::Tuple(values) if values.is_empty() => CoreExprKind::Unit,
            s::ExprKind::Tuple(values) => {
                CoreExprKind::Tuple(values.iter().map(|v| self.expr(v)).collect())
            }
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
                        CoreExprKind::IntegerConvert {
                            value: Box::new(self.expr(&arguments[0].value)),
                            target: match name {
                                "u8" => Type::U8,
                                "u16" => Type::U16,
                                "i32" => Type::I32,
                                "i64" => Type::I64,
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
                        let argument = self.type_ref(ty);
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
            s::ExprKind::Pipeline { input, target } => return self.pipeline(e.span, input, target),
            s::ExprKind::Member { object, member } => CoreExprKind::Member {
                object: Box::new(self.expr(object)),
                member: match member {
                    s::MemberName::Name(n) => n.clone(),
                    s::MemberName::TupleIndex(n) => n.to_string(),
                },
            },
            s::ExprKind::Record { ty, base, fields } => {
                return self.record(e.span, ty.as_deref(), base.as_deref(), fields, expected);
            }
            s::ExprKind::Closure {
                move_capture,
                params,
                return_type,
                body,
            } => {
                let parameters = params
                    .iter()
                    .map(|p| CoreClosureParameter {
                        name: p.name.clone(),
                        ty: p.ty.as_ref().map(|t| self.type_ref(t)),
                        mode: PassingMode::Infer,
                        span: p.span,
                    })
                    .collect();
                let saved_return = self.return_type.take();
                self.return_type =
                    return_type
                        .as_ref()
                        .map(|t| self.type_ref(t))
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
            s::ExprKind::If {
                condition,
                then_value,
                else_value,
            } => CoreExprKind::If {
                condition: Box::new(self.expr(condition)),
                then_value: Box::new(self.expr_expected(then_value, expected)),
                else_value: Box::new(self.expr_expected(else_value, expected)),
            },
            s::ExprKind::Try(inner) => CoreExprKind::Try(Box::new(self.expr(inner))),
            s::ExprKind::MetaEval(inner) => CoreExprKind::CompileTime(Box::new(self.expr(inner))),
            _ => {
                CoreExprKind::Unsupported("expression requires a later language capability".into())
            }
        };
        self.expr_node(kind, e.span)
    }
}
pub fn type_ref(t: &s::TypeRef) -> Type {
    match &t.kind {
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
            "c_wstr" => Type::CWideString,
            "i32" => Type::I32,
            "i64" => Type::I64,
            "bool" => Type::Bool,
            "void" => Type::Unit,
            n => Type::Named(n.into()),
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
        s::TypeKind::Reference { mutable, inner, .. } => Type::Address {
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
        s::TypeKind::Function { params, result } => Type::FunctionPointer {
            parameters: params.iter().map(type_ref).collect(),
            result: Box::new(type_ref(result)),
        },
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
