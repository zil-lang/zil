//! Borrow only resolved public project declarations. A declaration supplies a
//! signature, never conformance or permission to implement a foreign Self.
use super::*;
use ct_selection::Visitor;
use generics::Budget;
use zil_base::{FileId, ModuleId};

fn plain(ty: &s::TypeRef) -> Option<&str> {
    match &ty.kind {
        s::TypeKind::Named { path, args } if path.len() == 1 && args.is_empty() => Some(&path[0]),
        _ => None,
    }
}

// This is only a lazy-work switch during the already prepaid output scan.
// The actual owner map below uses full declarations and source identity.
pub(super) fn is_external_impl(item: &s::Item, current: ModuleId) -> bool {
    let s::ItemKind::Impl(implementation) = &item.kind else {
        return false;
    };
    implementation
        .trait_type
        .as_ref()
        .and_then(plain)
        .and_then(|name| name.strip_prefix("$module"))
        .and_then(|name| name.split_once('_'))
        .and_then(|(owner, _)| owner.parse::<u32>().ok())
        .is_some_and(|owner| owner != current.0)
}

pub(super) fn method_owners(
    specialized: &s::Module,
    modules: &[&s::Module],
    files: &BTreeMap<FileId, usize>,
    budget: &mut Budget<'_>,
) -> Result<BTreeMap<String, ModuleId>, Diagnostic> {
    let mut traits = BTreeMap::new();
    let mut hosts = BTreeMap::new();
    for item in &specialized.items {
        budget.node(item.span, 0)?;
        if !item.public {
            continue;
        }
        match &item.kind {
            s::ItemKind::Trait(t) if t.generics.is_empty() && t.bounds.is_empty() => {
                budget.text(&t.name, item.span)?;
                traits.insert(t.name.as_str(), item.span.file);
            }
            s::ItemKind::Aggregate(a)
                if a.generics.is_empty()
                    && matches!(
                        a.kind,
                        s::AggregateKind::Struct | s::AggregateKind::Resource
                    ) =>
            {
                budget.text(&a.name, item.span)?;
                hosts.insert(a.name.as_str(), item.span.file);
            }
            _ => {}
        }
    }
    let mut result = BTreeMap::new();
    for item in &specialized.items {
        budget.node(item.span, 0)?;
        let s::ItemKind::Impl(implementation) = &item.kind else {
            continue;
        };
        let (Some(trait_name), Some(host)) = (
            implementation.trait_type.as_ref().and_then(plain),
            plain(&implementation.target),
        ) else {
            continue;
        };
        budget.text(trait_name, item.span)?;
        budget.text(host, item.span)?;
        let (Some(trait_file), Some(host_file)) = (traits.get(trait_name), hosts.get(host)) else {
            continue;
        };
        if *host_file != item.span.file
            || trait_file == host_file
            || !implementation.generics.is_empty()
            || !implementation.where_clause.is_empty()
        {
            continue;
        }
        let position = *files.get(host_file).ok_or_else(|| {
            Diagnostic::error(
                Phase::Expand,
                "E-MODULE-ID",
                Some(item.span),
                "concrete trait implementation lost its source owner",
            )
        })?;
        budget.text(host, item.span)?;
        result.insert(host.to_owned(), modules[position].id);
    }
    Ok(result)
}

pub(super) fn instance_dependencies(
    item: &s::Item,
    owners: &BTreeMap<String, ModuleId>,
    current: ModuleId,
    edges: &mut Vec<CoreImport>,
    budget: &mut Budget<'_>,
) -> Result<(), Diagnostic> {
    struct Dependencies<'a, 'b, 'l> {
        owners: &'a BTreeMap<String, ModuleId>,
        current: ModuleId,
        edges: &'a mut Vec<CoreImport>,
        budget: &'b mut Budget<'l>,
    }
    impl Dependencies<'_, '_, '_> {
        fn nominal(&mut self, name: &str, span: Span) -> CompileResult<()> {
            self.text(name, span)?;
            let Some(owner) = self.owners.get(name).copied() else {
                return Ok(());
            };
            if owner == self.current {
                return Ok(());
            }
            for edge in self.edges.iter() {
                self.budget.node(span, 0).map_err(|e| vec![e])?;
                if edge.module == owner {
                    return Ok(());
                }
            }
            let digits = if owner.0 == 0 {
                1
            } else {
                owner.0.ilog10() as usize + 1
            };
            self.text("$module_import_", span)?;
            self.text(&"0000000000"[..digits], span)?;
            self.node(span, 0)?;
            let alias = format!("$module_import_{}", owner.0);
            self.edges.push(CoreImport {
                alias,
                module: owner,
                span,
            });
            Ok(())
        }
    }
    impl Visitor for Dependencies<'_, '_, '_> {
        fn node(&mut self, span: Span, depth: usize) -> CompileResult<()> {
            self.budget.node(span, depth).map_err(|e| vec![e])
        }
        fn text(&mut self, text: &str, span: Span) -> CompileResult<()> {
            self.budget.text(text, span).map_err(|e| vec![e])
        }
        fn ty(&mut self, ty: &s::TypeRef, depth: usize) -> CompileResult<()> {
            self.node(ty.span, depth)?;
            if let Some(name) = plain(ty) {
                self.nominal(name, ty.span)?;
            }
            ct_selection::walk_type(self, ty, depth)
        }
        fn expression(&mut self, expression: &s::Expr, depth: usize) -> CompileResult<()> {
            self.node(expression.span, depth)?;
            if let s::ExprKind::Name(name) = &expression.kind {
                self.nominal(name, expression.span)?;
            }
            ct_selection::walk_expression(self, expression, depth)
        }
    }
    Dependencies {
        owners,
        current,
        edges,
        budget,
    }
    .item(item, 0)
    .map_err(|mut errors| errors.remove(0))
}

fn spend(work: &mut usize, amount: usize, span: Span) -> Result<(), Diagnostic> {
    *work = work.checked_sub(amount).ok_or_else(|| {
        Diagnostic::limit(
            Phase::Expand,
            "project trait declaration budget exceeded",
            Some(span),
        )
    })?;
    Ok(())
}

struct Prepay<'a> {
    work: &'a mut usize,
    nodes: &'a mut usize,
    depth: usize,
}

impl Visitor for Prepay<'_> {
    fn node(&mut self, span: Span, depth: usize) -> CompileResult<()> {
        if depth > self.depth || *self.nodes == 0 {
            return Err(vec![Diagnostic::limit(
                Phase::Expand,
                "project trait syntax budget exceeded",
                Some(span),
            )]);
        }
        spend(self.work, 1, span).map_err(|e| vec![e])?;
        *self.nodes -= 1;
        Ok(())
    }

    fn text(&mut self, value: &str, span: Span) -> CompileResult<()> {
        spend(self.work, value.len(), span).map_err(|e| vec![e])
    }
}

pub(super) struct Lookup<'a> {
    pub current: zil_base::ModuleId,
    pub name: &'a str,
    pub span: Span,
}

pub(super) fn declaration<'a>(
    lookup: Lookup<'_>,
    declarations: &[&'a s::Module],
    imports: &[CoreImport],
    work: &mut usize,
    nodes: &mut usize,
    limits: &Limits,
) -> Result<Option<&'a s::Item>, Diagnostic> {
    let Lookup {
        current,
        name,
        span,
    } = lookup;
    let mut found = None;
    for module in declarations {
        spend(work, 1, span)?;
        if module.id == current {
            continue;
        }
        for item in &module.items {
            spend(work, 1, item.span)?;
            let s::ItemKind::Trait(trait_declaration) = &item.kind else {
                continue;
            };
            spend(
                work,
                name.len().saturating_add(trait_declaration.name.len()),
                span,
            )?;
            if trait_declaration.name != name {
                continue;
            }
            if found.is_some() {
                return Err(Diagnostic::error(
                    Phase::Expand,
                    "E-TRAIT-DUPLICATE",
                    Some(span),
                    "duplicate resolved project trait identity",
                ));
            }
            if !item.public {
                return Err(Diagnostic::error(
                    Phase::Expand,
                    "E-MODULE-PRIVATE",
                    Some(span),
                    "trait implementation requires a public imported declaration",
                ));
            }
            spend(work, imports.len(), span)?;
            if !imports.iter().any(|edge| edge.module == module.id) {
                return Err(Diagnostic::error(
                    Phase::Expand,
                    "E-MODULE-MEMBER",
                    Some(span),
                    "trait implementation declaration must belong to an imported module",
                ));
            }
            // Charge the full borrowed signature before any receiver, type,
            // name or index entry is copied by the existing conformance path.
            Prepay {
                work,
                nodes,
                depth: limits.max_call_depth,
            }
            .item(item, 0)
            .map_err(|mut e| e.remove(0))?;
            spend(work, name.len().saturating_mul(3), span)?;
            found = Some(item);
        }
    }
    Ok(found)
}
