//! Complete borrowed declaration views before specialization or generation.
//! These rows address actual syntax. They are not Contract-ready certificates.
//! In particular an abstract function and an empty block body stay distinct.
use crate::source_context::{DeclarationOrigin, ModuleDeclarationOrigins};
use zil_base::{CompileResult, Diagnostic, Limits, ModuleId, NodeId, Phase, Span};
use zil_syntax::ast::{self as s, ItemKind};
#[path = "source_declaration_trees.rs"]
mod trees;

/// A root or actual nested member in the original source namespace. The node
/// ordinal follows root items first, then actual fields/variants/members in
/// structural order, as in the Driver source symbol table. The root's original
/// origin and full path remain separate from this request-local address.
pub struct SourceDeclaration<'a> {
    pub module: ModuleId,
    pub declaration: NodeId,
    pub origin: &'a DeclarationOrigin,
    /// Placement in the actual materialized module, used only for emission.
    pub path: Vec<u32>,
    /// Structural path in its frozen original or generated source tree.
    pub source_path: Vec<u32>,
    pub item: &'a s::Item,
    pub parent: Option<usize>,
    pub public: bool,
}
impl SourceDeclaration<'_> {
    /// Original mixed order, including unused Type/Const/Life parameters.
    pub fn parameters(&self) -> &[s::GenericParam] {
        match &self.item.kind {
            ItemKind::Function(value)
            | ItemKind::Extern {
                function: value, ..
            } => &value.generics,
            ItemKind::Aggregate(value) => &value.generics,
            ItemKind::Enum(value) => &value.generics,
            ItemKind::Trait(value) => &value.generics,
            ItemKind::Impl(value) => &value.generics,
            ItemKind::TypeAlias(value) => &value.generics,
            ItemKind::Import(_) | ItemKind::Const(_) | ItemKind::Meta(_) => &[],
        }
    }
    /// Every original body, bound, default, receiver and contract remains in
    /// item; callers never substitute an empty CFG for an unresolved body.
    pub fn function(&self) -> Option<&s::Function> {
        match &self.item.kind {
            ItemKind::Function(value)
            | ItemKind::Extern {
                function: value, ..
            } => Some(value),
            _ => None,
        }
    }
    pub fn body(&self) -> Option<&s::FunctionBody> {
        let function = self.function()?;
        (!matches!(function.body, s::FunctionBody::Declaration)).then_some(&function.body)
    }
}

pub struct SourceDeclarationInventory<'a> {
    pub declarations: Vec<SourceDeclaration<'a>>,
    /// First free original node after roots, fields, variants and members.
    /// Body-local declaration allocation continues this same real namespace.
    pub next_by_module: Vec<(ModuleId, u32)>,
}

struct Budget<'a> {
    limits: &'a Limits,
    work: usize,
    bytes: usize,
    nodes: usize,
}
impl Budget<'_> {
    fn work(&mut self, count: usize, span: Span) -> CompileResult<()> {
        self.work = self.work.checked_sub(count).ok_or_else(|| {
            vec![Diagnostic::limit(
                Phase::Expand,
                "source declaration inventory work exhausted",
                Some(span),
            )]
        })?;
        Ok(())
    }
    fn bytes(&mut self, count: usize, span: Span) -> CompileResult<()> {
        self.bytes = self.bytes.checked_sub(count).ok_or_else(|| {
            vec![Diagnostic::limit(
                Phase::Expand,
                "source declaration inventory bytes exhausted",
                Some(span),
            )]
        })?;
        Ok(())
    }
    fn push<T>(&mut self, rows: &mut Vec<T>, value: T, span: Span) -> CompileResult<()> {
        self.work(1, span)?;
        if rows.len() == rows.capacity() {
            let growth = rows.capacity().max(4);
            self.work(rows.len(), span)?;
            self.bytes(
                growth
                    .checked_mul(std::mem::size_of::<T>())
                    .ok_or_else(|| {
                        failure(span, "source declaration inventory allocation overflow")
                    })?,
                span,
            )?;
            rows.try_reserve_exact(growth)
                .map_err(|_| failure(span, "source declaration inventory allocation failed"))?;
        }
        rows.push(value);
        Ok(())
    }
}
impl crate::ct_selection::Visitor for Budget<'_> {
    fn item(&mut self, item: &s::Item, depth: usize) -> CompileResult<()> {
        if let ItemKind::Function(function) | ItemKind::Extern { function, .. } = &item.kind
            && function.semantic.raw_index().is_some()
        {
            return Err(failure(
                function.span,
                "original function already contains a semantic owner token",
            ));
        }
        crate::ct_selection::walk_item(self, item, depth)
    }
    fn node(&mut self, span: Span, depth: usize) -> CompileResult<()> {
        self.work(1, span)?;
        self.nodes = self.nodes.checked_sub(1).ok_or_else(|| {
            vec![Diagnostic::limit(
                Phase::Expand,
                "source declaration inventory node budget exhausted",
                Some(span),
            )]
        })?;
        if depth >= self.limits.max_call_depth.min(128) {
            return Err(vec![Diagnostic::limit(
                Phase::Expand,
                "source declaration inventory depth exhausted",
                Some(span),
            )]);
        }
        Ok(())
    }
    fn text(&mut self, value: &str, span: Span) -> CompileResult<()> {
        self.work(value.len(), span)?;
        if crate::is_internal_source_type_name(value) {
            return Err(failure(
                span,
                "private resource category is not public Source syntax",
            ));
        }
        Ok(())
    }
    fn type_binder(&mut self, name: &str, span: Span) -> CompileResult<()> {
        if name == "Managed" {
            return Err(vec![crate::primitive_name_error(name, span)]);
        }
        Ok(())
    }
    fn expression(&mut self, expression: &s::Expr, depth: usize) -> CompileResult<()> {
        if expression.semantic.raw_index().is_some() {
            return Err(failure(
                expression.span,
                "original expression already contains a semantic production token",
            ));
        }
        crate::ct_selection::walk_expression(self, expression, depth)
    }
    fn statement(&mut self, statement: &s::Stmt, depth: usize) -> CompileResult<()> {
        if statement.semantic.raw_index().is_some() {
            return Err(failure(
                statement.span,
                "original declaration already contains a semantic production token",
            ));
        }
        crate::ct_selection::walk_statement(self, statement, depth)
    }
}
fn failure(span: Span, message: &str) -> Vec<Diagnostic> {
    vec![Diagnostic::error(
        Phase::Expand,
        "E-SOURCE-DECLARATIONS",
        Some(span),
        message,
    )]
}

fn header_matches(source: &s::Item, actual: &s::Item) -> bool {
    fn name(item: &s::Item) -> Option<&str> {
        match &item.kind {
            ItemKind::Function(f) | ItemKind::Extern { function: f, .. } => Some(&f.name),
            ItemKind::Aggregate(a) => Some(&a.name),
            ItemKind::Enum(e) => Some(&e.name),
            ItemKind::Trait(t) => Some(&t.name),
            ItemKind::TypeAlias(a) => Some(&a.name),
            ItemKind::Const(c) => Some(&c.name),
            _ => None,
        }
    }
    fn parameters(item: &s::Item) -> &[s::GenericParam] {
        match &item.kind {
            ItemKind::Function(f) | ItemKind::Extern { function: f, .. } => &f.generics,
            ItemKind::Aggregate(a) => &a.generics,
            ItemKind::Enum(e) => &e.generics,
            ItemKind::Trait(t) => &t.generics,
            ItemKind::Impl(i) => &i.generics,
            ItemKind::TypeAlias(a) => &a.generics,
            _ => &[],
        }
    }
    let formals = parameters(source);
    let actuals = parameters(actual);
    // CT may materialize types and bodies. It cannot move an actual declaration
    // to a different source binder or turn one declaration category into another.
    formals.len() == actuals.len()
        && formals.iter().zip(actuals).all(|(source, actual)| {
            source.name == actual.name
                && std::mem::discriminant(&source.kind) == std::mem::discriminant(&actual.kind)
        })
        && std::mem::discriminant(&source.kind) == std::mem::discriminant(&actual.kind)
        && name(source) == name(actual)
        && source.public == actual.public
        && !matches!((&source.kind, &actual.kind), (ItemKind::Aggregate(a), ItemKind::Aggregate(b)) if a.kind != b.kind)
}
struct Collector<'a, 'b, 'l> {
    module: ModuleId,
    origin: &'a DeclarationOrigin,
    tree: &'b trees::Tree<'a>,
    rows: &'b mut Vec<SourceDeclaration<'a>>,
    budget: &'b mut Budget<'l>,
}
impl<'a> Collector<'a, '_, '_> {
    fn member(
        &mut self,
        item: &'a s::Item,
        parent: usize,
        path: &mut Vec<u32>,
        source_path: &mut Vec<u32>,
        public: bool,
    ) -> CompileResult<()> {
        let (declaration, original) = self.tree.at(source_path, self.budget, item.span)?;
        self.budget
            .work(path.len().saturating_add(source_path.len()), item.span)?;
        self.budget.bytes(
            path.len()
                .saturating_add(source_path.len())
                .saturating_mul(4),
            item.span,
        )?;
        if !header_matches(original, item) {
            return Err(failure(
                item.span,
                "actual member changes its frozen source declaration",
            ));
        }
        let row = SourceDeclaration {
            module: self.module,
            declaration,
            origin: self.origin,
            path: path.clone(),
            source_path: source_path.clone(),
            item,
            parent: Some(parent),
            public,
        };
        let index = self.rows.len();
        self.budget.push(self.rows, row, item.span)?;
        self.children(item, original, index, path, source_path, public, None)
    }
    fn children(
        &mut self,
        item: &'a s::Item,
        original: &'a s::Item,
        parent: usize,
        path: &mut Vec<u32>,
        source_path: &mut Vec<u32>,
        public: bool,
        retained: Option<&crate::source_context::RetainedMemberOrigins>,
    ) -> CompileResult<()> {
        if path.len() >= self.budget.limits.max_call_depth.min(128) {
            return Err(vec![Diagnostic::limit(
                Phase::Expand,
                "source member path depth exhausted",
                Some(item.span),
            )]);
        }
        let (members, sources) = match (&item.kind, &original.kind) {
            (ItemKind::Aggregate(value), ItemKind::Aggregate(source)) => {
                self.budget.work(
                    value.fields.len().saturating_add(source.fields.len()),
                    item.span,
                )?;
                if value.fields.len() != source.fields.len()
                    || value
                        .fields
                        .iter()
                        .zip(&source.fields)
                        .any(|(actual, original)| actual.name != original.name)
                {
                    return Err(failure(
                        item.span,
                        "actual aggregate changes its frozen field namespace",
                    ));
                }
                if retained.is_some() && source.kind != s::AggregateKind::Struct {
                    return Err(failure(
                        item.span,
                        "retained member mapping requires an original struct",
                    ));
                }
                (&value.methods, &source.methods)
            }
            (ItemKind::Trait(value), ItemKind::Trait(source)) => (&value.members, &source.members),
            (ItemKind::Impl(value), ItemKind::Impl(source)) => (&value.members, &source.members),
            (ItemKind::Enum(value), ItemKind::Enum(source)) if retained.is_none() => {
                self.budget.work(
                    value.variants.len().saturating_add(source.variants.len()),
                    item.span,
                )?;
                if value.variants.len() != source.variants.len() {
                    return Err(failure(
                        item.span,
                        "actual enum changes its frozen variant namespace",
                    ));
                }
                for (actual, original) in value.variants.iter().zip(&source.variants) {
                    let same_fields = match (&actual.fields, &original.fields) {
                        (s::VariantFields::Unit, s::VariantFields::Unit) => true,
                        (s::VariantFields::Tuple(a), s::VariantFields::Tuple(b)) => {
                            a.len() == b.len()
                        }
                        (s::VariantFields::Record(a), s::VariantFields::Record(b)) => {
                            self.budget
                                .work(a.len().saturating_add(b.len()), actual.span)?;
                            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| a.name == b.name)
                        }
                        _ => false,
                    };
                    if actual.name != original.name || !same_fields {
                        return Err(failure(
                            actual.span,
                            "actual variant changes its frozen field namespace",
                        ));
                    }
                }
                return Ok(());
            }
            _ if retained.is_some() => {
                return Err(failure(
                    item.span,
                    "retained mapping has no original member vector",
                ));
            }
            _ => return Ok(()),
        };
        self.budget
            .work(members.len().saturating_add(sources.len()), item.span)?;
        if let Some(retained) = retained {
            if retained.original_count as usize != sources.len()
                || retained.positions.len() != members.len()
                || members.len() >= sources.len()
                || retained
                    .positions
                    .iter()
                    .any(|n| *n as usize >= sources.len())
                || retained.positions.windows(2).any(|p| p[0] >= p[1])
            {
                return Err(failure(
                    item.span,
                    "retained members change their complete original positions",
                ));
            }
        } else if members.len() != sources.len() {
            return Err(failure(
                item.span,
                "actual members changed without exact source correspondence",
            ));
        }
        for (ordinal, member) in members.iter().enumerate() {
            let source_ordinal = retained.map_or(ordinal as u32, |map| map.positions[ordinal]);
            self.budget.push(path, ordinal as u32, member.span)?;
            self.budget.push(source_path, source_ordinal, member.span)?;
            self.member(member, parent, path, source_path, public && member.public)?;
            source_path.pop();
            path.pop();
        }
        Ok(())
    }
}

/// Visit the whole immutable source inventory, not a filtered shape module.
/// The caller supplies aligned original identities captured before any AST
/// transformation. Returned limits continue the same request's ledger.
pub fn collect_original_declarations<'a>(
    modules: &[&'a s::Module],
    origins: &'a [ModuleDeclarationOrigins],
    limits: &Limits,
) -> CompileResult<(SourceDeclarationInventory<'a>, Limits)> {
    let mut remaining = limits.clone();
    let inventory = collect_original_declarations_with_budget(modules, origins, &mut remaining)?;
    Ok((inventory, remaining))
}

/// Debit the caller's bounded inventory allowance on both success and failure.
/// The returned rows borrow immutable syntax; no readiness or permission is
/// cached in this pure source bridge.
pub fn collect_original_declarations_with_budget<'a>(
    modules: &[&'a s::Module],
    origins: &'a [ModuleDeclarationOrigins],
    remaining: &mut Limits,
) -> CompileResult<SourceDeclarationInventory<'a>> {
    let limits = remaining.clone();
    let mut budget = Budget {
        limits: &limits,
        work: remaining.max_analysis_steps,
        bytes: remaining.max_source_bytes,
        nodes: remaining.max_nodes,
    };
    let result = collect_inner(modules, origins, &mut budget);
    remaining.max_analysis_steps = budget.work;
    remaining.max_source_bytes = budget.bytes;
    remaining.max_nodes = budget.nodes;
    result
}

/// Count the full structural namespace, including non-item fields and
/// variants, before a materializer gives a new tree its module-local IDs.
pub fn source_declaration_count(module: &s::Module, remaining: &mut Limits) -> CompileResult<u32> {
    let limits = remaining.clone();
    let mut budget = Budget {
        limits: &limits,
        work: remaining.max_analysis_steps,
        bytes: remaining.max_source_bytes,
        nodes: remaining.max_nodes,
    };
    let result = trees::Tree::new(module, None, &mut budget).map(|tree| tree.count);
    remaining.max_analysis_steps = budget.work;
    remaining.max_source_bytes = budget.bytes;
    remaining.max_nodes = budget.nodes;
    result
}

fn collect_inner<'a>(
    modules: &[&'a s::Module],
    origins: &'a [ModuleDeclarationOrigins],
    budget: &mut Budget<'_>,
) -> CompileResult<SourceDeclarationInventory<'a>> {
    use crate::ct_selection::Visitor;
    let span = modules
        .first()
        .map_or_else(Span::default, |module| module.span);
    budget.work(modules.len().saturating_add(origins.len()), span)?;
    if modules.len() != origins.len() {
        return Err(failure(
            span,
            "original source module inventory is incomplete",
        ));
    }
    let mut declarations = Vec::new();
    let mut next_by_module = Vec::new();
    let mut seen = Vec::new();
    for module in modules {
        budget.work(seen.len().saturating_add(origins.len()), module.span)?;
        if seen.contains(&module.id) {
            return Err(failure(module.span, "original source module is duplicated"));
        }
        budget.push(&mut seen, module.id, module.span)?;
        let origin = origins
            .iter()
            .find(|origin| origin.module == module.id)
            .ok_or_else(|| failure(module.span, "original source module origins are absent"))?;
        if origin.items.len() != module.items.len() {
            return Err(failure(
                module.span,
                "original source item inventory is incomplete",
            ));
        }
        for item in &module.items {
            budget.item(item, 0)?;
        }
        let namespace = trees::Namespace::new(origin, budget)?;
        let roots = declarations.len();
        let mut actual_ids = std::collections::BTreeSet::new();
        for (ordinal, (item, row)) in module.items.iter().zip(&origin.items).enumerate() {
            let actual = u32::try_from(ordinal)
                .map_err(|_| failure(item.span, "actual declaration ordinal overflow"))?;
            let (tree, source_root) = namespace.root(row, budget, item.span)?;
            let (declaration, source) = tree.at(&[source_root], budget, item.span)?;
            if declaration != row.declaration || !header_matches(source, item) {
                return Err(failure(
                    item.span,
                    "actual root changes its exact frozen source declaration",
                ));
            }
            budget.work(
                actual_ids.len().checked_ilog2().unwrap_or(0) as usize + 1,
                item.span,
            )?;
            budget.bytes(48, item.span)?;
            if !actual_ids.insert(declaration) {
                return Err(failure(
                    item.span,
                    "actual output repeats an original declaration",
                ));
            }
            budget.bytes(8, item.span)?;
            budget.push(
                &mut declarations,
                SourceDeclaration {
                    module: module.id,
                    declaration,
                    origin: row,
                    path: vec![actual],
                    source_path: vec![source_root],
                    item,
                    parent: None,
                    public: item.public,
                },
                item.span,
            )?;
        }
        for (ordinal, item) in module.items.iter().enumerate() {
            let row = &origin.items[ordinal];
            let (tree, source_root) = namespace.root(row, budget, item.span)?;
            let (_, source) = tree.at(&[source_root], budget, item.span)?;
            let mut path = Vec::new();
            let mut source_path = Vec::new();
            budget.push(&mut path, ordinal as u32, item.span)?;
            budget.push(&mut source_path, source_root, item.span)?;
            let mut collector = Collector {
                module: module.id,
                origin: row,
                tree,
                rows: &mut declarations,
                budget,
            };
            collector.children(
                item,
                source,
                roots + ordinal,
                &mut path,
                &mut source_path,
                item.public,
                row.retained_members.as_ref(),
            )?;
        }
        budget.push(
            &mut next_by_module,
            (module.id, namespace.next),
            module.span,
        )?;
    }
    Ok(SourceDeclarationInventory {
        declarations,
        next_by_module,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use zil_base::{FileId, SourceUnit};

    #[test]
    fn failed_inventory_attempts_debit_the_shared_ledger_and_never_publish_rows() {
        let (module, mut origins) = source("fn identity<T>(value:T)->T{return value}");
        let limits = Limits::default();
        origins[0].items.clear();
        let mut remaining = limits.clone();
        for _ in 0..2 {
            let before = remaining.max_analysis_steps;
            assert!(
                collect_original_declarations_with_budget(&[&module], &origins, &mut remaining)
                    .is_err()
            );
            assert!(
                remaining.max_analysis_steps < before,
                "failed attempts do actual paid source work"
            );
        }
        let (module, origins) = source("fn identity<T>(value:T)->T{return value}");
        let mut remaining = Limits {
            max_nodes: 0,
            ..limits.clone()
        };
        let errors =
            match collect_original_declarations_with_budget(&[&module], &origins, &mut remaining) {
                Ok(_) => panic!("zero node allowance produced a declaration inventory"),
                Err(errors) => errors,
            };
        assert!(
            errors.iter().any(|d| d.code == "E-RESOURCE-LIMIT"),
            "{errors:?}"
        );
        let mut fresh = limits.clone();
        let inventory =
            collect_original_declarations_with_budget(&[&module], &origins, &mut fresh).unwrap();
        assert_eq!(inventory.declarations.len(), 1);
        assert!(inventory.declarations[0].body().is_some());
        assert_eq!(inventory.declarations[0].parameters().len(), 1);
    }

    fn source(text: &str) -> (s::Module, Vec<ModuleDeclarationOrigins>) {
        let source = SourceUnit::new(FileId(0), ModuleId(0), "source.zil", text);
        let module = zil_syntax::parse(&source).into_result().unwrap();
        let (origins, _) = crate::source_context::capture_original_required(
            &[source],
            &[&module],
            crate::source_context::SourceIdentityContext::ClosedSnapshot {
                root: ModuleId(0),
                aliases: &BTreeMap::new(),
                declarations: crate::source_context::DeclarationPlacement::Original,
            },
            &Limits::default(),
        )
        .unwrap();
        (module, origins)
    }

    #[test]
    fn original_mixed_formals_associated_defaults_and_real_bodies_are_borrowed_whole() {
        let (module, origins) = source(
            "pub trait Read<life p,#N:usize,T>{type Item:Copy;const COUNT:usize=42;fn read<life q,U>(self:&Self,value:U)->U{return value}}struct Cell<T>{value:T;fn number()->i32{return 42}}fn abstracted();",
        );
        let limits = Limits::default();
        let (inventory, _) = collect_original_declarations(&[&module], &origins, &limits).unwrap();
        let root = &inventory.declarations[0];
        assert_eq!(root.parameters().len(), 3);
        assert!(matches!(
            root.parameters()[0].kind,
            s::GenericParamKind::Lifetime
        ));
        assert!(matches!(
            root.parameters()[1].kind,
            s::GenericParamKind::Const(_)
        ));
        assert!(std::ptr::eq(root.item, &module.items[0]));
        let method = inventory
            .declarations
            .iter()
            .find(|row| row.function().is_some_and(|f| f.name == "read"))
            .unwrap();
        assert_eq!(method.path, [0, 2]);
        assert_eq!(method.origin.declaration, NodeId(0));
        assert_eq!(method.parent, Some(0));
        assert_eq!(method.parameters().len(), 2);
        assert!(matches!(method.body(), Some(s::FunctionBody::Block(_))));
        let number = inventory
            .declarations
            .iter()
            .find(|row| row.function().is_some_and(|f| f.name == "number"))
            .unwrap();
        assert_eq!(number.declaration, NodeId(7));
        assert_eq!(number.path, [1, 0]);
        assert!(!number.public);
        let declaration = inventory
            .declarations
            .iter()
            .find(|row| row.function().is_some_and(|f| f.name == "abstracted"))
            .unwrap();
        assert!(declaration.body().is_none());
        assert!(matches!(
            declaration.function().unwrap().body,
            s::FunctionBody::Declaration
        ));
        let associated = inventory
            .declarations
            .iter()
            .filter(|row| matches!(row.item.kind, ItemKind::TypeAlias(_) | ItemKind::Const(_)))
            .count();
        assert_eq!(associated, 2);
    }

    #[test]
    fn transformed_origins_and_missing_declarations_do_not_become_original_inventory() {
        let (module, mut origins) = source("fn first(){}fn second(){}");
        let limits = Limits::default();
        origins[0].items.pop();
        assert!(collect_original_declarations(&[&module], &origins, &limits).is_err());
        let (module, mut origins) = source("struct Host{fn first(){}fn second(){}}");
        origins[0].items[0].retained_members = Some(crate::source_context::RetainedMemberOrigins {
            original_count: 3,
            positions: vec![0, 2],
        });
        assert!(collect_original_declarations(&[&module], &origins, &limits).is_err());
    }

    #[test]
    fn original_declaration_discovery_has_exact_repeatable_request_budget() {
        let (module, origins) =
            source("struct Host<T>{value:T;fn first<life p>(value:&'p T)->&'p T{return value}}");
        let limits = Limits::default();
        let (_, remaining) = collect_original_declarations(&[&module], &origins, &limits).unwrap();
        let spent = limits.max_analysis_steps - remaining.max_analysis_steps;
        assert!(spent > 0);
        for _ in 0..2 {
            let exact = Limits {
                max_analysis_steps: spent,
                ..limits.clone()
            };
            let (_, remaining) =
                collect_original_declarations(&[&module], &origins, &exact).unwrap();
            assert_eq!(remaining.max_analysis_steps, 0);
            let short = Limits {
                max_analysis_steps: spent - 1,
                ..limits.clone()
            };
            let errors = collect_original_declarations(&[&module], &origins, &short)
                .err()
                .unwrap();
            assert!(
                errors
                    .iter()
                    .all(|d| d.category == zil_base::ResultCategory::ResourceLimit)
            );
        }
    }

    #[test]
    fn moved_roots_and_retained_members_keep_frozen_source_ordinals() {
        let (mut module, mut origins) = source(
            "fn removed(){} struct Host{x:i32;fn dropped()->i32{return 1}fn kept()->i32{return 2}} fn main()->i32{return 0}",
        );
        let frozen = origins[0].original.clone();
        module.items.remove(0);
        origins[0].items.remove(0);
        let ItemKind::Aggregate(host) = &mut module.items[0].kind else {
            panic!()
        };
        host.methods.remove(0);
        origins[0].items[0].retained_members = Some(crate::source_context::RetainedMemberOrigins {
            original_count: 2,
            positions: vec![1],
        });
        let (inventory, _) =
            collect_original_declarations(&[&module], &origins, &Limits::default()).unwrap();
        let kept = inventory
            .declarations
            .iter()
            .find(|row| row.function().is_some_and(|f| f.name == "kept"))
            .unwrap();
        assert_eq!(kept.declaration, NodeId(5));
        assert_eq!(kept.path, [0, 0]);
        assert_eq!(kept.source_path, [1, 1]);
        assert_eq!(inventory.next_by_module, [(ModuleId(0), 6)]);
        assert!(std::sync::Arc::ptr_eq(&frozen, &origins[0].original));
        assert_eq!(frozen.items.len(), 3);
        let ItemKind::Aggregate(host) = &frozen.items[1].kind else {
            panic!()
        };
        assert_eq!(host.methods.len(), 2);
    }

    #[test]
    fn nested_generation_keeps_consumed_emitter_tree_and_all_declaration_ids() {
        use crate::source_context::{
            ClosureSiteLineage, GeneratedDeclarationOrigin, GeneratedSourceTree,
            SourceDeclarationSite, SourceTreeIdentity,
        };
        use std::sync::Arc;
        let (mut module, mut origins) = source("#emit factory()\nfn main()->i32{return 0}");
        let (parent, _) = source("#emit child_factory()");
        let (child, _) =
            source("struct Made{x:i32;fn value()->i32{return 7}} fn answer()->i32{return 7}");
        let parent = Arc::new(GeneratedSourceTree {
            identity: SourceTreeIdentity::Generated {
                module: ModuleId(0),
                event_path: vec![0, 0],
            },
            emitter: SourceDeclarationSite {
                tree: SourceTreeIdentity::Original(ModuleId(0)),
                declaration: NodeId(0),
                event_path: vec![0],
            },
            module: Arc::new(parent),
            declarations: vec![NodeId(2)],
        });
        let child = Arc::new(GeneratedSourceTree {
            identity: SourceTreeIdentity::Generated {
                module: ModuleId(0),
                event_path: vec![0, 0, 1, 0],
            },
            emitter: SourceDeclarationSite {
                tree: parent.identity.clone(),
                declaration: NodeId(2),
                event_path: vec![0],
            },
            module: Arc::new(child),
            declarations: (3..7).map(NodeId).collect(),
        });
        let namespace = origins[0].items[0].identity.clone();
        let generated = |root| DeclarationOrigin {
            closure_sites: ClosureSiteLineage::Unavailable,
            declaration: child.declarations[root as usize],
            identity: child.root_identity(root, &namespace).unwrap(),
            retained_members: None,
            generated: Some(GeneratedDeclarationOrigin {
                tree: child.clone(),
                root,
            }),
        };
        // Place the generated function first although its source root is one.
        module.items = vec![
            child.module.items[1].clone(),
            child.module.items[0].clone(),
            module.items[1].clone(),
        ];
        origins[0].items = vec![generated(1), generated(0), origins[0].items[1].clone()];
        origins[0].generated.trees = vec![parent.clone(), child.clone()];
        let (inventory, _) =
            collect_original_declarations(&[&module], &origins, &Limits::default()).unwrap();
        assert_eq!(inventory.declarations[0].declaration, NodeId(4));
        assert_eq!(inventory.declarations[0].path, [0]);
        assert_eq!(inventory.declarations[0].source_path, [1]);
        let method = inventory
            .declarations
            .iter()
            .find(|row| row.function().is_some_and(|f| f.name == "value"))
            .unwrap();
        assert_eq!(method.declaration, NodeId(6));
        assert_eq!(method.path, [1, 0]);
        assert_eq!(method.source_path, [0, 0]);
        assert_eq!(inventory.next_by_module, [(ModuleId(0), 7)]);
        let (prepared, program) = crate::source_semantics::program::prepare(
            &[&module],
            &origins,
            &[Vec::new()],
            &mut Limits::default(),
        )
        .unwrap();
        let ItemKind::Function(answer) = &prepared[0].items[0].kind else {
            panic!()
        };
        assert!(answer.semantic.raw_index().is_some());
        assert!(
            program.modules[0]
                .1
                .definitions
                .iter()
                .any(|definition| definition.id.index == 4)
        );
        origins[0].generated.trees.swap(0, 1);
        assert!(collect_original_declarations(&[&module], &origins, &Limits::default()).is_ok());
        origins[0].generated.trees.swap(0, 1);
        origins[0].generated.trees.remove(0);
        assert!(collect_original_declarations(&[&module], &origins, &Limits::default()).is_err());
        origins[0].generated.trees.insert(0, parent);
        let mut forged = (*child).clone();
        forged.emitter.declaration = NodeId(1);
        origins[0].generated.trees[1] = Arc::new(forged);
        assert!(collect_original_declarations(&[&module], &origins, &Limits::default()).is_err());
        origins[0].generated.trees[1] = child.clone();
        origins[0].items[0].declaration = NodeId(3);
        assert!(collect_original_declarations(&[&module], &origins, &Limits::default()).is_err());
    }
}
