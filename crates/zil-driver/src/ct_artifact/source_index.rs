//! Frozen source identities for the CT compiler and read views. A symbol is an
//! index into actual parsed source, never a capability supplied by raw CTIR.
use super::compiler::{ImportTarget, SourceImport, SourceModule};
use std::collections::{BTreeMap, BTreeSet};
use zil_base::{CompileResult, Diagnostic, ModuleId, NodeId, Phase, SourceUnit, Span};
use zil_ir::ct_artifact::CtSymbolId;
use zil_syntax::ast::{self as s, ItemKind, VariantFields};

pub(super) struct SourceIndex {
    module_ids: Vec<ModuleId>,
    imports: Vec<Vec<SourceImport>>,
    symbols: Vec<Vec<(Span, CtSymbolId)>>,
    public: Vec<(CtSymbolId, bool)>,
}

fn error(code: &str, span: Span, message: impl Into<String>) -> Vec<Diagnostic> {
    vec![Diagnostic::error(Phase::Ct, code, Some(span), message)]
}

fn add(
    symbols: &mut Vec<(Span, CtSymbolId)>,
    public: &mut Vec<(CtSymbolId, bool)>,
    module: ModuleId,
    span: Span,
    visible: bool,
    next: &mut u32,
) -> CompileResult<()> {
    let declaration = NodeId(*next);
    *next = next.checked_add(1).ok_or_else(|| {
        error(
            "E-CT-SYMBOL-LIMIT",
            span,
            "source declaration count overflow",
        )
    })?;
    let symbol = CtSymbolId {
        module,
        declaration,
        generation_path: None,
    };
    public.push((symbol.clone(), visible));
    symbols.push((span, symbol));
    Ok(())
}

fn children(
    item: &s::Item,
    module: ModuleId,
    symbols: &mut Vec<(Span, CtSymbolId)>,
    public: &mut Vec<(CtSymbolId, bool)>,
    next: &mut u32,
    owner_visible: bool,
) -> CompileResult<()> {
    match &item.kind {
        ItemKind::Aggregate(aggregate) => {
            for field in &aggregate.fields {
                add(
                    symbols,
                    public,
                    module,
                    field.span,
                    owner_visible && field.public,
                    next,
                )?;
            }
            for member in &aggregate.methods {
                add(
                    symbols,
                    public,
                    module,
                    member.span,
                    owner_visible && member.public,
                    next,
                )?;
                children(
                    member,
                    module,
                    symbols,
                    public,
                    next,
                    owner_visible && member.public,
                )?;
            }
        }
        ItemKind::Enum(enumeration) => {
            for variant in &enumeration.variants {
                add(symbols, public, module, variant.span, owner_visible, next)?;
                match &variant.fields {
                    VariantFields::Unit => {}
                    VariantFields::Tuple(fields) => {
                        for field in fields {
                            add(symbols, public, module, field.span, owner_visible, next)?;
                        }
                    }
                    VariantFields::Record(fields) => {
                        for field in fields {
                            add(
                                symbols,
                                public,
                                module,
                                field.span,
                                owner_visible && field.public,
                                next,
                            )?;
                        }
                    }
                }
            }
        }
        ItemKind::Trait(declaration) => {
            for member in &declaration.members {
                add(
                    symbols,
                    public,
                    module,
                    member.span,
                    owner_visible && member.public,
                    next,
                )?;
                children(
                    member,
                    module,
                    symbols,
                    public,
                    next,
                    owner_visible && member.public,
                )?;
            }
        }
        ItemKind::Impl(declaration) => {
            for member in &declaration.members {
                add(
                    symbols,
                    public,
                    module,
                    member.span,
                    owner_visible && member.public,
                    next,
                )?;
                children(
                    member,
                    module,
                    symbols,
                    public,
                    next,
                    owner_visible && member.public,
                )?;
            }
        }
        _ => {}
    }
    Ok(())
}

impl SourceIndex {
    /// A private draft gets an independently paid complete index. No caller
    /// can observe inserted rows before all generated definitions validate.
    pub(super) fn copy_for_generation(
        &self,
        budget: &mut zil_ct::CtExecutionBudget,
    ) -> CompileResult<Self> {
        let mut bytes = self.module_ids.len() as u64 * std::mem::size_of::<ModuleId>() as u64
            + (self.imports.len() + self.symbols.len()) as u64
                * std::mem::size_of::<Vec<SourceImport>>() as u64
            + self.public.len() as u64 * std::mem::size_of::<(CtSymbolId, bool)>() as u64;
        budget.reserve_analysis_work(
            self.module_ids.len() as u64
                + self.imports.len() as u64
                + self.symbols.len() as u64
                + self.public.len() as u64,
        )?;
        for imports in &self.imports {
            budget.reserve_analysis_work(imports.len() as u64)?;
            bytes = bytes
                .saturating_add(imports.len() as u64 * std::mem::size_of::<SourceImport>() as u64);
            for import in imports {
                budget.reserve_analysis_work(import.alias.len() as u64)?;
                bytes = bytes.saturating_add(import.alias.len() as u64);
            }
        }
        for symbols in &self.symbols {
            budget.reserve_analysis_work(symbols.len() as u64)?;
            bytes = bytes.saturating_add(
                symbols.len() as u64 * std::mem::size_of::<(Span, CtSymbolId)>() as u64,
            );
        }
        budget.reserve_temporary_bytes(bytes)?;
        Ok(Self {
            module_ids: self.module_ids.clone(),
            imports: self.imports.clone(),
            symbols: self.symbols.clone(),
            public: self.public.clone(),
        })
    }
    /// Register the actual frozen event inventory. The Materializer owns the
    /// verified emission; this only adds its precise source bindings and normal
    /// public/private flags, never type, scope, reflection, or loan authority.
    pub(super) fn register_generated_tree(
        &mut self,
        tree: &zil_expand::source_context::GeneratedSourceTree,
        limits: &zil_base::Limits,
        budget: &mut zil_ct::CtExecutionBudget,
    ) -> CompileResult<()> {
        let span = tree.module.span;
        budget.reserve_analysis_work(
            self.module_ids.len() as u64 + tree.identity_path_len() as u64 + 1,
        )?;
        let owner = self
            .module_ids
            .iter()
            .position(|module| *module == tree.module.id)
            .ok_or_else(|| error("E-CT-SOURCE-INDEX", span, "generated source module absent"))?;
        let fingerprint = tree.generation_fingerprint().ok_or_else(|| {
            error(
                "E-CT-SOURCE-INDEX",
                span,
                "generated source tree identity absent",
            )
        })?;
        let cost = zil_expand::ct_selection::module_cost(&tree.module, limits)?;
        budget.reserve_analysis_work(cost.work as u64)?;
        let count = tree.declarations.len();
        budget.reserve_analysis_work(
            (count as u64).saturating_mul(self.public.len() as u64 + count as u64 + 1),
        )?;
        budget.reserve_generated_nodes(cost.nodes as u64)?;
        // Temporary local inventory and final enlarged destinations are paid
        // before either allocation. The temporary vector is released on success.
        let entry_bytes =
            std::mem::size_of::<(Span, CtSymbolId)>() + std::mem::size_of::<(CtSymbolId, bool)>();
        let temporary = cost.nodes as u64 * entry_bytes as u64;
        budget.reserve_temporary_bytes(
            temporary
                + (self.symbols[owner].len() + count) as u64
                    * std::mem::size_of::<(Span, CtSymbolId)>() as u64
                + (self.public.len() + count) as u64
                    * std::mem::size_of::<(CtSymbolId, bool)>() as u64,
        )?;
        let mut symbols = Vec::with_capacity(cost.nodes);
        let mut public = Vec::with_capacity(cost.nodes);
        for (local, item) in tree.module.items.iter().enumerate() {
            let symbol = CtSymbolId {
                module: tree.module.id,
                declaration: NodeId(local as u32),
                generation_path: None,
            };
            symbols.push((item.span, symbol.clone()));
            public.push((symbol, item.public));
        }
        let mut next = u32::try_from(tree.module.items.len())
            .map_err(|_| error("E-CT-SYMBOL-LIMIT", span, "generated root count overflow"))?;
        for item in &tree.module.items {
            children(
                item,
                tree.module.id,
                &mut symbols,
                &mut public,
                &mut next,
                item.public,
            )?;
        }
        if next as usize != count || symbols.len() != count || public.len() != count {
            return Err(error(
                "E-CT-SOURCE-INDEX",
                span,
                "generated frozen declaration inventory count differs",
            ));
        }
        for (ordinal, ((_, symbol), (visible_symbol, _))) in
            symbols.iter_mut().zip(&mut public).enumerate()
        {
            let declaration = tree.declarations[ordinal];
            if tree.declarations[..ordinal].contains(&declaration)
                || self
                    .public
                    .iter()
                    .any(|(old, _)| old.module == tree.module.id && old.declaration == declaration)
            {
                return Err(error(
                    "E-CT-SOURCE-INDEX",
                    span,
                    "generated declaration ID collides with existing source inventory",
                ));
            }
            symbol.declaration = declaration;
            symbol.generation_path = Some(fingerprint);
            *visible_symbol = symbol.clone();
        }
        self.symbols[owner].reserve_exact(count);
        self.public.reserve_exact(count);
        self.symbols[owner].extend(symbols);
        self.public.extend(public);
        budget.release_temporary_bytes(temporary);
        Ok(())
    }

    #[cfg(test)]
    pub(super) fn freeze(
        sources: &[SourceUnit],
        syntax: &[&s::Module],
        paths: &BTreeMap<Vec<String>, ModuleId>,
    ) -> CompileResult<Self> {
        Self::freeze_scoped(sources, syntax, paths, None)
    }

    pub(super) fn freeze_scoped(
        sources: &[SourceUnit],
        syntax: &[&s::Module],
        paths: &BTreeMap<Vec<String>, ModuleId>,
        scoped: Option<&crate::ScopedImports>,
    ) -> CompileResult<Self> {
        if sources.len() != syntax.len() {
            return Err(vec![Diagnostic::error(
                Phase::Ct,
                "E-CT-SOURCE-INDEX",
                None,
                "source and syntax counts differ",
            )]);
        }
        let mut imports = Vec::with_capacity(sources.len());
        let mut symbols = Vec::with_capacity(sources.len());
        let mut public = Vec::new();
        for (source, module) in sources.iter().zip(syntax) {
            if source.module_id != module.id {
                return Err(error(
                    "E-CT-SOURCE-INDEX",
                    module.span,
                    "parsed module identity differs from source",
                ));
            }
            let mut bindings = Vec::new();
            let mut used = BTreeSet::new();
            for item in &module.items {
                let ItemKind::Import(import) = &item.kind else {
                    continue;
                };
                let Some(alias) = import.alias.clone().or_else(|| import.path.last().cloned())
                else {
                    return Err(error("E-CT-IMPORT", item.span, "empty import path"));
                };
                if !used.insert(alias.clone()) {
                    return Err(error(
                        "E-CT-IMPORT-ALIAS",
                        item.span,
                        "duplicate module alias",
                    ));
                }
                if import.syntax {
                    continue;
                }
                let target = match import.path.as_slice() {
                    [a, b] if a == "std" && b == "meta" => ImportTarget::Meta,
                    [a, b] if a == "std" && b == "syntax" => ImportTarget::Syntax,
                    _ => ImportTarget::Module(
                        *scoped
                            .filter(|_| import.path.first().is_none_or(|part| part != "std"))
                            .and_then(|map| map.get(&(module.id, import.path.clone())))
                            .or_else(|| paths.get(&import.path))
                            .ok_or_else(|| {
                                error("E-CT-IMPORT", item.span, "import has no frozen module")
                            })?,
                    ),
                };
                bindings.push(SourceImport { alias, target });
            }
            let mut table = Vec::new();
            let mut next = u32::try_from(module.items.len()).map_err(|_| {
                error(
                    "E-CT-SYMBOL-LIMIT",
                    module.span,
                    "source declaration count overflow",
                )
            })?;
            for (index, item) in module.items.iter().enumerate() {
                let symbol = CtSymbolId {
                    module: module.id,
                    declaration: NodeId(u32::try_from(index).map_err(|_| {
                        error(
                            "E-CT-SYMBOL-LIMIT",
                            item.span,
                            "source declaration index overflow",
                        )
                    })?),
                    generation_path: None,
                };
                public.push((symbol.clone(), item.public));
                table.push((item.span, symbol));
            }
            for item in &module.items {
                children(
                    item,
                    module.id,
                    &mut table,
                    &mut public,
                    &mut next,
                    item.public,
                )?;
            }
            imports.push(bindings);
            symbols.push(table);
        }
        Ok(Self {
            module_ids: sources.iter().map(|source| source.module_id).collect(),
            imports,
            symbols,
            public,
        })
    }

    pub(super) fn aliases(&self, requester: ModuleId) -> BTreeMap<ModuleId, String> {
        let mut result = BTreeMap::new();
        if let Some(index) = self
            .module_ids
            .iter()
            .position(|module| *module == requester)
        {
            for import in &self.imports[index] {
                if let ImportTarget::Module(target) = &import.target {
                    result
                        .entry(*target)
                        .or_insert_with(|| import.alias.clone());
                }
            }
        }
        result
    }

    pub(super) fn imported_module(&self, requester: ModuleId, alias: &str) -> Option<ModuleId> {
        let index = self
            .module_ids
            .iter()
            .position(|module| *module == requester)?;
        self.imports[index]
            .iter()
            .find_map(|import| match &import.target {
                ImportTarget::Module(target) if import.alias == alias => Some(*target),
                _ => None,
            })
    }

    pub(super) fn alias_taken(&self, requester: ModuleId, alias: &str) -> bool {
        self.module_ids
            .iter()
            .position(|module| *module == requester)
            .is_some_and(|index| {
                self.imports[index]
                    .iter()
                    .any(|import| import.alias == alias)
            })
    }

    pub(super) fn modules<'a>(
        &'a self,
        sources: &'a [SourceUnit],
        syntax: &'a [&s::Module],
    ) -> Vec<SourceModule<'a>> {
        sources
            .iter()
            .zip(syntax)
            .enumerate()
            .map(|(index, (source, module))| SourceModule {
                source,
                syntax: module,
                imports: &self.imports[index],
                symbols: &self.symbols[index],
            })
            .collect()
    }

    pub(super) fn contains(&self, symbol: &CtSymbolId) -> bool {
        self.symbols
            .iter()
            .flatten()
            .any(|(_, candidate)| candidate == symbol)
    }

    pub(super) fn span(&self, symbol: &CtSymbolId) -> Option<Span> {
        self.symbols
            .iter()
            .flatten()
            .find_map(|(span, candidate)| (candidate == symbol).then_some(*span))
    }

    pub(super) fn symbol_lookup_work(&self) -> u64 {
        (self.module_ids.len() as u64).saturating_add(self.public.len() as u64)
    }

    pub(super) fn symbol_at(&self, module: ModuleId, span: Span) -> Option<CtSymbolId> {
        self.symbols.iter().flatten().find_map(|(actual, symbol)| {
            (symbol.module == module && *actual == span).then(|| symbol.clone())
        })
    }

    pub(super) fn visible(&self, requester: &CtSymbolId, symbol: &CtSymbolId) -> bool {
        self.contains(requester)
            && self.public.iter().any(|(candidate, public)| {
                candidate == symbol && (requester.module == symbol.module || *public)
            })
    }
}
