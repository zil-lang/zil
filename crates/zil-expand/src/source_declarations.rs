//! Complete borrowed declaration views before specialization or generation.
//! These rows address actual syntax. They are not Contract-ready certificates.
//! In particular an abstract function and an empty block body stay distinct.
use crate::source_context::{DeclarationOrigin, ModuleDeclarationOrigins};
use zil_base::{CompileResult, Diagnostic, Limits, ModuleId, NodeId, Phase, Span};
use zil_syntax::ast::{self as s, ItemKind};

/// A root or actual nested member in the original source namespace. The node
/// ordinal follows root items first, then actual fields/variants/members in
/// structural order, as in the Driver source symbol table. The root's original
/// origin and full path remain separate from this request-local address.
pub struct SourceDeclaration<'a> {
    pub module: ModuleId,
    pub declaration: NodeId,
    pub origin: &'a DeclarationOrigin,
    pub path: Vec<u32>,
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

struct Collector<'a, 'b, 'l> {
    module: ModuleId,
    origin: &'a DeclarationOrigin,
    next: &'b mut u32,
    rows: &'b mut Vec<SourceDeclaration<'a>>,
    budget: &'b mut Budget<'l>,
}
impl<'a> Collector<'a, '_, '_> {
    fn allocate(&mut self, count: usize, span: Span) -> CompileResult<u32> {
        self.budget.work(count.saturating_add(1), span)?;
        let id = *self.next;
        *self.next = self
            .next
            .checked_add(
                u32::try_from(count)
                    .map_err(|_| failure(span, "source member ordinal overflow"))?,
            )
            .ok_or_else(|| failure(span, "source member ordinal overflow"))?;
        Ok(id)
    }
    fn member(
        &mut self,
        item: &'a s::Item,
        parent: usize,
        path: &mut Vec<u32>,
        public: bool,
    ) -> CompileResult<()> {
        let declaration = NodeId(self.allocate(1, item.span)?);
        self.budget.work(path.len(), item.span)?;
        self.budget.bytes(path.len().saturating_mul(4), item.span)?;
        let row = SourceDeclaration {
            module: self.module,
            declaration,
            origin: self.origin,
            path: path.clone(),
            item,
            parent: Some(parent),
            public,
        };
        let index = self.rows.len();
        self.budget.push(self.rows, row, item.span)?;
        self.children(item, index, path, public)
    }
    fn children(
        &mut self,
        item: &'a s::Item,
        parent: usize,
        path: &mut Vec<u32>,
        public: bool,
    ) -> CompileResult<()> {
        if path.len() >= self.budget.limits.max_call_depth.min(128) {
            return Err(vec![Diagnostic::limit(
                Phase::Expand,
                "source member path depth exhausted",
                Some(item.span),
            )]);
        }
        let members = match &item.kind {
            ItemKind::Aggregate(value) => {
                self.allocate(value.fields.len(), item.span)?;
                &value.methods
            }
            ItemKind::Trait(value) => &value.members,
            ItemKind::Impl(value) => &value.members,
            ItemKind::Enum(value) => {
                for variant in &value.variants {
                    self.allocate(1, variant.span)?;
                    let count = match &variant.fields {
                        s::VariantFields::Unit => 0,
                        s::VariantFields::Tuple(fields) => fields.len(),
                        s::VariantFields::Record(fields) => fields.len(),
                    };
                    self.allocate(count, variant.span)?;
                }
                return Ok(());
            }
            _ => return Ok(()),
        };
        for (index, member) in members.iter().enumerate() {
            let index = u32::try_from(index)
                .map_err(|_| failure(member.span, "source member ordinal overflow"))?;
            self.budget.push(path, index, member.span)?;
            self.member(member, parent, path, public && member.public)?;
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
        let roots = declarations.len();
        for (ordinal, (item, origin)) in module.items.iter().zip(&origin.items).enumerate() {
            let id = u32::try_from(ordinal)
                .map_err(|_| failure(item.span, "original declaration ordinal overflow"))?;
            if origin.declaration != NodeId(id)
                || origin.retained_members.is_some()
                || origin.generated.is_some()
            {
                return Err(failure(
                    item.span,
                    "original source declaration placement was transformed",
                ));
            }
            budget.bytes(4, item.span)?;
            budget.push(
                &mut declarations,
                SourceDeclaration {
                    module: module.id,
                    declaration: NodeId(id),
                    origin,
                    path: vec![id],
                    item,
                    parent: None,
                    public: item.public,
                },
                item.span,
            )?;
        }
        let mut next = u32::try_from(module.items.len())
            .map_err(|_| failure(module.span, "original declaration ordinal overflow"))?;
        let mut path = Vec::new();
        for (ordinal, item) in module.items.iter().enumerate() {
            budget.push(&mut path, ordinal as u32, item.span)?;
            let mut collector = Collector {
                module: module.id,
                origin: &origin.items[ordinal],
                next: &mut next,
                rows: &mut declarations,
                budget,
            };
            collector.children(item, roots + ordinal, &mut path, item.public)?;
            path.pop();
        }
        budget.push(&mut next_by_module, (module.id, next), module.span)?;
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
}
