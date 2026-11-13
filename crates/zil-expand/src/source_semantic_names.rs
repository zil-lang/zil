//! Original declaration bindings. This directory contains no inferred value
//! types, SSA occupants, provider certificates or lookup by source spans.
use super::*;
impl Builder<'_, '_, '_, '_> {
    fn original_name(
        &mut self,
        names: &mut Vec<SemanticOriginalNameBinding>,
        path: Vec<String>,
        namespace: SemanticOriginalNamespace,
        value: SemanticOriginalNameValue,
        shadowed: bool,
        span: Span,
    ) -> R<()> {
        self.budget
            .identity_work(names.len().saturating_mul(path.len() + 1), span)?;
        if let Some(previous) = names
            .iter()
            .find(|old| old.namespace == namespace && old.path == path)
        {
            if shadowed || previous.value == value {
                return Ok(());
            }
            return Err(error(
                span,
                "original declaration name has ambiguous bindings",
            ));
        }
        self.push(
            names,
            SemanticOriginalNameBinding {
                path,
                namespace,
                value,
            },
            span,
        )
    }
    fn name_path(&mut self, prefix: Option<&str>, name: &str, span: Span) -> R<Vec<String>> {
        let mut path = Vec::new();
        if let Some(prefix) = prefix {
            let prefix = self.text(prefix, span)?;
            self.push(&mut path, prefix, span)?;
        }
        let name = self.text(name, span)?;
        self.push(&mut path, name, span)?;
        Ok(path)
    }
    fn original_bound_methods(
        &mut self,
        names: &mut Vec<SemanticOriginalNameBinding>,
        receiver: &str,
        bound: &s::TypeRef,
    ) -> R<()> {
        self.budget.node(bound.span, 0)?;
        let mut bound = bound;
        while let s::TypeKind::Group(inner) | s::TypeKind::ForLifetime { inner, .. } = &bound.kind {
            bound = inner;
            self.budget.node(bound.span, 0)?;
        }
        if self.builtin_trait(bound)?.is_some() {
            return Ok(());
        }
        let s::TypeKind::Named { path, .. } = &bound.kind else {
            return Ok(());
        };
        if matches!(path.as_slice(),[name]if matches!(name.as_str(),"Fn"|"FnMut"|"FnOnce")) {
            return Ok(());
        }
        let owner = self.named(path, bound.span)?;
        let inventory = self.inventory;
        let s::ItemKind::Trait(declaration) = &inventory.declarations[owner].item.kind else {
            return Err(error(
                bound.span,
                "original member bound does not name a trait declaration",
            ));
        };
        for (ordinal, item) in declaration.members.iter().enumerate() {
            self.budget.node(item.span, 0)?;
            let s::ItemKind::Function(method) = &item.kind else {
                continue;
            };
            let ordinal = u32::try_from(ordinal)
                .map_err(|_| error(item.span, "original method ordinal overflow"))?;
            let mut selected = None;
            for (index, row) in inventory.declarations.iter().enumerate() {
                self.budget.node(row.item.span, 0)?;
                if row.parent == Some(owner) && row.path.last() == Some(&ordinal) {
                    if selected.replace(index).is_some() {
                        return Err(error(
                            item.span,
                            "original trait member has ambiguous structural identity",
                        ));
                    }
                }
            }
            let member = selected.ok_or_else(|| {
                error(
                    item.span,
                    "original trait member has no structural declaration identity",
                )
            })?;
            let target = SemanticOriginalNameValue::Definition(self.id(member));
            let mut qualified = Vec::new();
            let name = self.text(receiver, item.span)?;
            self.push(&mut qualified, name, item.span)?;
            for component in path {
                let name = self.text(component, item.span)?;
                self.push(&mut qualified, name, item.span)?;
            }
            let name = self.text(&method.name, item.span)?;
            self.push(&mut qualified, name, item.span)?;
            self.original_name(
                names,
                qualified,
                SemanticOriginalNamespace::Value,
                target.clone(),
                false,
                item.span,
            )?;
            let path = self.name_path(Some(receiver), &method.name, item.span)?;
            let ambiguous = names.iter().any(|row| {
                row.namespace == SemanticOriginalNamespace::Value
                    && row.path.len() > 2
                    && row.path.first().is_some_and(|name| name == receiver)
                    && row.path.last() == Some(&method.name)
                    && row.value != target
            });
            if ambiguous {
                if let Some(index) = names.iter().position(|row| {
                    row.namespace == SemanticOriginalNamespace::Value && row.path == path
                }) {
                    names.remove(index);
                }
            } else if let Some(index) = names.iter().position(|row| {
                row.namespace == SemanticOriginalNamespace::Value
                    && row.path == path
                    && row.value != target
            }) {
                // Distinct declared traits can expose the same member. Keep
                // both qualified original bindings and leave unqualified
                // selection to the caller's actual premise solver.
                names.remove(index);
            } else {
                self.original_name(
                    names,
                    path,
                    SemanticOriginalNamespace::Value,
                    target,
                    false,
                    item.span,
                )?;
            }
        }
        Ok(())
    }
    pub(super) fn original_names(&mut self) -> R<Vec<SemanticOriginalNameBinding>> {
        let inventory = self.inventory;
        let owner = self.owner;
        let mut names = Vec::new();
        let mut current = Some(owner);
        let mut distance = self.lexical_offset;
        while let Some(index) = current {
            let row = &inventory.declarations[index];
            for (ordinal, formal) in row.parameters().iter().enumerate() {
                let reference = SemanticFormalRef {
                    scope: SemanticScopeRef::Ancestor(distance),
                    ordinal: u32::try_from(ordinal)
                        .map_err(|_| error(formal.span, "original name formal ordinal overflow"))?,
                };
                let (namespace, recipe) = match &formal.kind {
                    s::GenericParamKind::Lifetime => (
                        SemanticOriginalNamespace::Lifetime,
                        SemanticActualRecipe::Life(SemanticLifeRecipe::Formal(reference.clone())),
                    ),
                    s::GenericParamKind::Type(_) => (
                        SemanticOriginalNamespace::Type,
                        SemanticActualRecipe::Type(SemanticTypeRecipe::Formal(reference.clone())),
                    ),
                    s::GenericParamKind::Const(carrier) => {
                        let saved_owner = self.owner;
                        let saved_offset = self.lexical_offset;
                        self.owner = index;
                        self.lexical_offset = distance;
                        let carrier = self.ty(carrier, &mut vec![0, ordinal as u32], 0, false);
                        self.owner = saved_owner;
                        self.lexical_offset = saved_offset;
                        (
                            SemanticOriginalNamespace::Value,
                            SemanticActualRecipe::Const {
                                carrier: carrier?,
                                value: SemanticConstRecipe::Formal(reference.clone()),
                            },
                        )
                    }
                };
                if let s::GenericParamKind::Type(bounds) = &formal.kind
                    && self.formal(&formal.name, 0, formal.span)? == Some(reference)
                {
                    for bound in bounds {
                        self.original_bound_methods(&mut names, &formal.name, bound)?;
                    }
                }
                let path = self.name_path(None, &formal.name, formal.span)?;
                self.original_name(
                    &mut names,
                    path,
                    namespace,
                    SemanticOriginalNameValue::Recipe(recipe),
                    true,
                    formal.span,
                )?;
            }
            let clauses = match &row.item.kind {
                s::ItemKind::Function(value)
                | s::ItemKind::Extern {
                    function: value, ..
                } => value.where_clause.as_slice(),
                s::ItemKind::Impl(value) => value.where_clause.as_slice(),
                _ => &[],
            };
            for clause in clauses {
                if let s::BoundSubject::Type(subject) = &clause.subject
                    && let s::TypeKind::Named { path, args } = &subject.kind
                    && args.is_empty()
                    && let [name] = path.as_slice()
                    && self
                        .formal(name, 0, subject.span)?
                        .is_some_and(|reference| {
                            reference.scope == SemanticScopeRef::Ancestor(distance)
                        })
                {
                    for bound in &clause.bounds {
                        self.original_bound_methods(&mut names, name, bound)?;
                    }
                }
            }
            current = row.parent;
            distance = distance
                .checked_add(1)
                .ok_or_else(|| error(row.item.span, "original name lexical depth overflow"))?;
        }
        let module = inventory.declarations[owner].module;
        // Module/import directories are actual source bindings. A nearer
        // formal shadows them; value parameters and locals are handled by the
        // complete original body, never entered here as fake constants.
        for (index, row) in inventory.declarations.iter().enumerate() {
            self.budget.node(row.item.span, 0)?;
            if row.parent.is_some() {
                continue;
            }
            let (spelling, namespace) = match &row.item.kind {
                s::ItemKind::Function(function) | s::ItemKind::Extern { function, .. } => {
                    (&function.name, SemanticOriginalNamespace::Value)
                }
                s::ItemKind::Aggregate(value) => (&value.name, SemanticOriginalNamespace::Type),
                s::ItemKind::Enum(value) => (&value.name, SemanticOriginalNamespace::Type),
                s::ItemKind::Trait(value) => (&value.name, SemanticOriginalNamespace::Type),
                // Aliases are normalized by their actual source recipe when
                // used as a type. A global Const has no original definition
                // carrier in this model and cannot be invented as a function.
                _ => continue,
            };
            if row.module == module {
                let path = self.name_path(None, spelling, row.item.span)?;
                self.original_name(
                    &mut names,
                    path,
                    namespace,
                    SemanticOriginalNameValue::Definition(self.id(index)),
                    true,
                    row.item.span,
                )?;
            } else if row.public {
                for (origin, edges) in self.imports {
                    if *origin != module {
                        continue;
                    }
                    for edge in *edges {
                        self.budget.node(edge.span, 0)?;
                        if edge.module != row.module {
                            continue;
                        }
                        let path = self.name_path(Some(&edge.alias), spelling, row.item.span)?;
                        self.original_name(
                            &mut names,
                            path,
                            namespace,
                            SemanticOriginalNameValue::Definition(self.id(index)),
                            false,
                            row.item.span,
                        )?;
                    }
                }
            }
        }
        let mut ambiguous_methods = std::collections::BTreeSet::new();
        // Method spellings come from the actual original member AST. The
        // receiver's symbolic type and complete impl premises still choose
        // an applicable method; these bindings select no concrete provider.
        for (member, row) in inventory.declarations.iter().enumerate() {
            let Some(parent) = row.parent else {
                continue;
            };
            let Some(method) = row.function() else {
                continue;
            };
            let scope = &inventory.declarations[parent];
            let owner_path = match &scope.item.kind {
                s::ItemKind::Aggregate(value) => std::slice::from_ref(&value.name),
                s::ItemKind::Trait(value) => std::slice::from_ref(&value.name),
                s::ItemKind::Impl(value) => {
                    let mut ty = &value.target;
                    while let s::TypeKind::Group(inner) = &ty.kind {
                        ty = inner;
                        self.budget.node(ty.span, 0)?;
                    }
                    let s::TypeKind::Named { path, .. } = &ty.kind else {
                        continue;
                    };
                    path.as_slice()
                }
                _ => continue,
            };
            let mut prefixes = Vec::new();
            if row.module == module {
                self.push(&mut prefixes, None, row.item.span)?;
            } else if row.public {
                for (origin, edges) in self.imports {
                    if *origin == module {
                        for edge in *edges {
                            if edge.module == row.module {
                                self.push(&mut prefixes, Some(edge.alias.as_str()), edge.span)?;
                            }
                        }
                    }
                }
            }
            if !prefixes.is_empty() {
                // A length-delimited structural scope identity is candidate
                // data, not a new source binder. The full original identity
                // and actual method spelling remain available even when
                // multiple impls expose the same convenience Owner.method.
                let identity = self.identity(member)?;
                let mut candidate = Vec::new();
                let package = self.text(&identity.package, row.item.span)?;
                self.push(&mut candidate, package, row.item.span)?;
                for (label, parts) in [
                    ("module", identity.module_path.as_slice()),
                    ("declaration", identity.declaration_path.as_slice()),
                ] {
                    let tag = self.text(&format!("@{label}:{}", parts.len()), row.item.span)?;
                    self.push(&mut candidate, tag, row.item.span)?;
                    for part in parts {
                        let text = self.text(part, row.item.span)?;
                        self.push(&mut candidate, text, row.item.span)?;
                    }
                }
                let tag = self.text(
                    &format!("@generation:{}", identity.generation_path.len()),
                    row.item.span,
                )?;
                self.push(&mut candidate, tag, row.item.span)?;
                for generation in &identity.generation_path {
                    let text = self.text(&generation.to_string(), row.item.span)?;
                    self.push(&mut candidate, text, row.item.span)?;
                }
                let name = self.text(&method.name, row.item.span)?;
                self.push(&mut candidate, name, row.item.span)?;
                self.original_name(
                    &mut names,
                    candidate,
                    SemanticOriginalNamespace::Value,
                    SemanticOriginalNameValue::Definition(self.id(member)),
                    false,
                    row.item.span,
                )?;
            }
            for prefix in prefixes {
                if prefix.is_none()
                    && let Some(first) = owner_path.first()
                    && self.formal(first, 0, row.item.span)?.is_some()
                {
                    continue;
                }
                let mut path = Vec::new();
                if let Some(prefix) = prefix {
                    let name = self.text(prefix, row.item.span)?;
                    self.push(&mut path, name, row.item.span)?;
                }
                for name in owner_path {
                    let name = self.text(name, row.item.span)?;
                    self.push(&mut path, name, row.item.span)?;
                }
                let name = self.text(&method.name, row.item.span)?;
                self.push(&mut path, name, row.item.span)?;
                let target = SemanticOriginalNameValue::Definition(self.id(member));
                self.budget
                    .identity_work(names.len().saturating_mul(path.len() + 1), row.item.span)?;
                if ambiguous_methods.contains(&path) {
                    continue;
                }
                if let Some(index) = names.iter().position(|row| {
                    row.namespace == SemanticOriginalNamespace::Value
                        && row.path == path
                        && row.value != target
                }) {
                    names.remove(index);
                    self.budget
                        .lineage_bytes(12 * std::mem::size_of::<Vec<String>>(), row.item.span)?;
                    ambiguous_methods.insert(path);
                } else {
                    self.original_name(
                        &mut names,
                        path,
                        SemanticOriginalNamespace::Value,
                        target,
                        false,
                        row.item.span,
                    )?;
                }
            }
        }
        for nominal in self.compiler_nominals {
            self.budget
                .node(inventory.declarations[owner].item.span, 0)?;
            if nominal.module != module {
                continue;
            }
            let span = inventory.declarations[owner].item.span;
            let mut path = Vec::new();
            for component in nominal.name.split('.') {
                let component = self.text(component, span)?;
                self.push(&mut path, component, span)?;
            }
            self.original_name(
                &mut names,
                path,
                SemanticOriginalNamespace::Type,
                SemanticOriginalNameValue::Definition(nominal.definition),
                true,
                span,
            )?;
        }
        Ok(names)
    }
}
