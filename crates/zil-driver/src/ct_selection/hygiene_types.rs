//! Type-space hygiene for finite generated declarations. The old function-only
//! path does not construct these tables or pay for their traversal.
use super::*;
use std::collections::BTreeSet;

pub(super) struct TypeNames {
    pub names: BTreeMap<String, String>,
    pub parameters: BTreeSet<String>,
    modules: BTreeSet<String>,
}

impl Renamer<'_> {
    pub(super) fn type_reference(&mut self, name: &mut String, span: Span) -> CompileResult<()> {
        let Some(types) = &self.types else {
            return Ok(());
        };
        self.budget.spend(
            0,
            name.len().saturating_mul(
                types
                    .names
                    .len()
                    .saturating_add(types.parameters.len())
                    .saturating_add(2),
            ),
            span,
        )?;
        if types.parameters.contains(name) {
            return Ok(());
        }
        if let Some(replacement) = types.names.get(name) {
            self.budget.text(replacement, span)?;
            *name = replacement.clone();
        }
        Ok(())
    }

    // A nominal alias can be the left side of an enum constructor or static
    // method. Value binders win in this expression position; module qualifiers
    // retain their original identity even beside a same-spelled generated type.
    pub(super) fn type_namespace(
        &mut self,
        value: &mut s::Expr,
        depth: usize,
    ) -> CompileResult<bool> {
        let Some(types) = &self.types else {
            return Ok(false);
        };
        match &mut value.kind {
            s::ExprKind::Name(name) => {
                self.budget.spend(
                    0,
                    name.len().saturating_mul(
                        types
                            .modules
                            .len()
                            .saturating_add(types.names.len())
                            .saturating_add(2),
                    ),
                    value.span,
                )?;
                for scope in self.scopes.iter().rev() {
                    self.budget.spend(
                        0,
                        name.len().saturating_mul(scope.len().saturating_add(1)),
                        value.span,
                    )?;
                    if scope.contains_key(name) {
                        return Ok(false);
                    }
                }
                if types.modules.contains(name) {
                    self.budget.node(value.span, depth)?;
                    return Ok(true);
                }
                if !types.names.contains_key(name) || types.parameters.contains(name) {
                    return Ok(false);
                }
                self.budget.node(value.span, depth)?;
                self.type_reference(name, value.span)?;
                Ok(true)
            }
            s::ExprKind::Group(inner) => {
                self.budget.node(value.span, depth)?;
                self.type_namespace(inner, depth + 1)
            }
            _ => Ok(false),
        }
    }

    fn attributes(&mut self, attributes: &mut [s::Attribute], depth: usize) -> CompileResult<()> {
        for attribute in attributes {
            self.budget.node(attribute.span, depth)?;
            for argument in &mut attribute.arguments {
                self.expression(argument, depth + 1)?;
            }
        }
        Ok(())
    }

    fn contract_types(&mut self, term: &mut s::ContractTerm, depth: usize) -> CompileResult<()> {
        self.budget.node(term.span, depth)?;
        for argument in &mut term.type_args {
            self.argument(argument, depth + 1)?;
        }
        for argument in &mut term.arguments {
            self.contract_types(argument, depth + 1)?;
        }
        if let Some(index) = &mut term.index {
            self.contract_types(index, depth + 1)?;
        }
        Ok(())
    }

    pub(super) fn function_type_scope(
        &mut self,
        function: &mut s::Function,
        depth: usize,
    ) -> CompileResult<()> {
        let Some(types) = &mut self.types else {
            return Ok(());
        };
        // Register every type binder before visiting even the first bound.
        // Const/lifetime binders do not shadow the nominal type namespace.
        for parameter in &function.generics {
            if matches!(parameter.kind, s::GenericParamKind::Type(_)) {
                self.budget.spend(
                    1,
                    parameter
                        .name
                        .len()
                        .saturating_mul(types.parameters.len().saturating_add(2)),
                    parameter.span,
                )?;
                types.parameters.insert(parameter.name.clone());
            }
        }
        for parameter in &mut function.generics {
            self.budget.node(parameter.span, depth)?;
            match &mut parameter.kind {
                s::GenericParamKind::Type(bounds) => {
                    for bound in bounds {
                        self.ty(bound, depth + 1)?;
                    }
                }
                s::GenericParamKind::Const(ty) => self.ty(ty, depth + 1)?,
                s::GenericParamKind::Lifetime => {}
            }
        }
        for parameter in &mut function.params {
            self.attributes(&mut parameter.attributes, depth)?;
        }
        for bound in &mut function.where_clause {
            self.budget.node(bound.span, depth)?;
            if let s::BoundSubject::Type(ty) = &mut bound.subject {
                self.ty(ty, depth + 1)?;
            }
            for ty in &mut bound.bounds {
                self.ty(ty, depth + 1)?;
            }
        }
        for clause in &mut function.contract {
            self.budget.node(clause.span, depth)?;
            for term in &mut clause.terms {
                self.contract_types(term, depth + 1)?;
            }
        }
        Ok(())
    }

    pub(super) fn type_items(
        &mut self,
        items: &mut [s::Item],
        imports: &[CoreImport],
    ) -> CompileResult<()> {
        let span = items[0].span;
        self.budget.spend(1, 1, span)?;
        let mut modules = BTreeSet::new();
        for import in imports {
            self.budget.spend(
                1,
                import
                    .alias
                    .len()
                    .saturating_mul(modules.len().saturating_add(2)),
                import.span,
            )?;
            modules.insert(import.alias.clone());
        }
        self.types = Some(TypeNames {
            names: BTreeMap::new(),
            parameters: BTreeSet::new(),
            modules,
        });
        let mut declarations = BTreeSet::new();
        // Validate the entire batch and reserve source names before changing
        // any spelling; duplicates must not become different fresh identities.
        for item in &*items {
            self.budget.node(item.span, 0)?;
            let name = match &item.kind {
                s::ItemKind::Function(function) => &function.name,
                s::ItemKind::Aggregate(aggregate)
                    if aggregate.kind == s::AggregateKind::Struct
                        && aggregate.generics.is_empty()
                        && aggregate.methods.is_empty() =>
                {
                    &aggregate.name
                }
                s::ItemKind::TypeAlias(alias)
                    if alias.generics.is_empty()
                        && alias.bounds.is_empty()
                        && alias.value.is_some() =>
                {
                    &alias.name
                }
                _ => return Err(self.unsupported(item.span)),
            };
            self.budget.spend(
                1,
                name.len()
                    .saturating_mul(declarations.len().saturating_add(2)),
                item.span,
            )?;
            if !declarations.insert(name.clone()) {
                return Err(vec![Diagnostic::error(
                    Phase::Expand,
                    "E-NAME-DUPLICATE",
                    Some(item.span),
                    "duplicate declaration name in one generated iteration",
                )]);
            }
            if matches!(item.kind, s::ItemKind::Function(_)) {
                zil_expand::check_generated_function_name(name, item.span)?;
            } else {
                zil_expand::check_generated_type_name(name, item.span)?;
            }
        }
        for item in &mut *items {
            match &mut item.kind {
                s::ItemKind::Function(function) => {
                    self.bind(&mut function.name, !item.public, item.span)?
                }
                s::ItemKind::Aggregate(aggregate) => {
                    self.bind_type(&mut aggregate.name, !item.public, item.span)?
                }
                s::ItemKind::TypeAlias(alias) => {
                    self.bind_type(&mut alias.name, !item.public, item.span)?
                }
                _ => unreachable!(),
            }
        }
        for item in items {
            self.budget.node(item.span, 0)?;
            self.attributes(&mut item.attributes, 1)?;
            match &mut item.kind {
                s::ItemKind::Function(function) => self.function(function, 1)?,
                s::ItemKind::Aggregate(aggregate) => {
                    for field in &mut aggregate.fields {
                        self.budget.node(field.span, 1)?;
                        self.attributes(&mut field.attributes, 2)?;
                        self.ty(&mut field.ty, 2)?;
                        if let Some(value) = &mut field.default {
                            self.expression(value, 2)?;
                        }
                    }
                }
                s::ItemKind::TypeAlias(alias) => {
                    self.ty(alias.value.as_mut().expect("validated alias"), 1)?
                }
                _ => unreachable!(),
            }
        }
        Ok(())
    }

    fn bind_type(&mut self, name: &mut String, rename: bool, span: Span) -> CompileResult<()> {
        // Reuse the exact fresh-name allocator without putting type bindings
        // into the value namespace or duplicating the charging protocol.
        let mut names = std::mem::take(&mut self.types.as_mut().expect("type batch").names);
        std::mem::swap(&mut self.scopes[0], &mut names);
        let result = self.bind(name, rename, span);
        std::mem::swap(&mut self.scopes[0], &mut names);
        self.types.as_mut().expect("type batch").names = names;
        result
    }
}
