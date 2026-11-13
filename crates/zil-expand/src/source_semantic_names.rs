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
                        SemanticActualRecipe::Life(SemanticLifeRecipe::Formal(reference)),
                    ),
                    s::GenericParamKind::Type(_) => (
                        SemanticOriginalNamespace::Type,
                        SemanticActualRecipe::Type(SemanticTypeRecipe::Formal(reference)),
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
                                value: SemanticConstRecipe::Formal(reference),
                            },
                        )
                    }
                };
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
