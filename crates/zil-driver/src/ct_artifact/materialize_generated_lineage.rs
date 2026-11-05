//! Actual ItemList events and their immutable pre-remap declaration trees.
use super::*;
use zil_expand::source_context::{
    ClosureSiteLineage, DeclarationOrigin, GeneratedDeclarationOrigin, GeneratedSourceTree,
    SourceTreeIdentity,
};

pub(super) enum OutputOrigin {
    Original(usize),
    Generated(Box<DeclarationOrigin>),
}

fn children_count(
    item: &s::Item,
    depth: usize,
    max_depth: usize,
    budget: &mut zil_ct::CtExecutionBudget,
) -> CompileResult<usize> {
    budget.reserve_analysis_work(1)?;
    if depth > max_depth {
        return Err(vec![Diagnostic::limit(
            Phase::Ct,
            "generated declaration inventory depth exhausted",
            Some(item.span),
        )]);
    }
    let (mut count, members) = match &item.kind {
        s::ItemKind::Aggregate(host) => (host.fields.len(), host.methods.as_slice()),
        s::ItemKind::Trait(host) => (0, host.members.as_slice()),
        s::ItemKind::Impl(host) => (0, host.members.as_slice()),
        s::ItemKind::Enum(host) => {
            let mut count = host.variants.len();
            budget.reserve_analysis_work(host.variants.len() as u64)?;
            for variant in &host.variants {
                count = count
                    .checked_add(match &variant.fields {
                        s::VariantFields::Unit => 0,
                        s::VariantFields::Tuple(fields) => fields.len(),
                        s::VariantFields::Record(fields) => fields.len(),
                    })
                    .ok_or_else(|| category("generated declaration count overflow", item.span))?;
            }
            (count, &[][..])
        }
        _ => (0, &[][..]),
    };
    for member in members {
        count = count
            .checked_add(1)
            .ok_or_else(|| category("generated declaration count overflow", item.span))?;
        count = count
            .checked_add(children_count(member, depth + 1, max_depth, budget)?)
            .ok_or_else(|| category("generated declaration count overflow", item.span))?;
    }
    budget.reserve_analysis_work(count as u64)?;
    Ok(count)
}

impl Materializer<'_, '_> {
    pub(super) fn reserve_output_origins(
        &mut self,
        rows: &mut Vec<OutputOrigin>,
        additional: usize,
    ) -> CompileResult<()> {
        let required = rows
            .len()
            .checked_add(additional)
            .ok_or_else(|| category("output origin count overflow", Span::default()))?;
        self.environment
            .budget
            .reserve_analysis_work(additional as u64)?;
        if required > rows.capacity() {
            let old = rows.capacity();
            let new_capacity = required.max(old.checked_mul(2).unwrap_or(required));
            self.environment
                .budget
                .reserve_analysis_work(rows.len() as u64)?;
            self.environment.budget.reserve_temporary_bytes(
                new_capacity as u64 * std::mem::size_of::<OutputOrigin>() as u64,
            )?;
            rows.reserve_exact(new_capacity - rows.len());
            self.environment
                .budget
                .release_temporary_bytes(old as u64 * std::mem::size_of::<OutputOrigin>() as u64);
        }
        Ok(())
    }

    pub(super) fn prepay_source_lookup(&mut self, symbol: &CtSymbolId) -> CompileResult<()> {
        let work = if symbol.generation_path.is_some() {
            self.environment
                .generated_catalog
                .trees
                .iter()
                .fold(1u64, |n, tree| {
                    n.saturating_add(
                        tree.declarations.len() as u64 * 4 + tree.identity_path_len() as u64 + 1,
                    )
                })
        } else {
            let top = self
                .parsed
                .iter()
                .find(|parsed| parsed.module.id == symbol.module)
                .is_some_and(|parsed| (symbol.declaration.0 as usize) < parsed.module.items.len());
            self.parsed.len() as u64
                + if top {
                    1
                } else {
                    self.environment
                        .index
                        .symbol_lookup_work()
                        .saturating_mul(2)
                }
        };
        self.environment.budget.reserve_analysis_work(work)
    }

    pub(super) fn source_declaration<'a>(
        &'a self,
        symbol: &CtSymbolId,
        span: Span,
    ) -> CompileResult<&'a s::Item> {
        if let Some(fingerprint) = symbol.generation_path {
            let tree = self
                .environment
                .generated_catalog
                .trees
                .iter()
                .find(|tree| {
                    tree.module.id == symbol.module
                        && tree.generation_fingerprint() == Some(fingerprint)
                        && tree.declarations.contains(&symbol.declaration)
                })
                .ok_or_else(|| category("generated nominal source tree absent", span))?;
            let local = tree
                .declarations
                .iter()
                .position(|id| *id == symbol.declaration)
                .ok_or_else(|| category("generated nominal source declaration absent", span))?;
            return compiler::source_scope::original_declaration(
                &tree.module,
                zil_base::NodeId(local as u32),
                self.max_depth,
                |_| Ok(()),
            )?
            .map(|source| source.item)
            .ok_or_else(|| category("generated nominal has no actual declaration item", span));
        }
        let (_, parsed) = self.parsed_source(symbol.module)?;
        compiler::source_scope::original_declaration(
            &parsed.module,
            symbol.declaration,
            self.max_depth,
            |_| Ok(()),
        )?
        .map(|source| source.item)
        .ok_or_else(|| category("nominal has no actual original declaration item", span))
    }

    pub(super) fn source_lifetime_name(
        &mut self,
        symbol: &CtSymbolId,
        ordinal: u32,
        span: Span,
    ) -> CompileResult<String> {
        self.prepay_source_lookup(symbol)?;
        let source = self.source_declaration(symbol, span)?;
        let parameters = match &source.kind {
            s::ItemKind::Function(f) | s::ItemKind::Extern { function: f, .. } => &f.generics,
            s::ItemKind::Aggregate(a) => &a.generics,
            s::ItemKind::Enum(e) => &e.generics,
            s::ItemKind::Trait(t) => &t.generics,
            s::ItemKind::Impl(i) => &i.generics,
            _ => {
                return Err(category(
                    "source lifetime owner has no actual binder header",
                    span,
                ));
            }
        };
        let parameter = parameters
            .get(ordinal as usize)
            .ok_or_else(|| category("source lifetime full ordinal is absent", span))?;
        if !matches!(parameter.kind, s::GenericParamKind::Lifetime) {
            return Err(category(
                "source lifetime ordinal selects another binder kind",
                span,
            ));
        }
        let bytes = parameter.name.len();
        self.environment
            .budget
            .reserve_analysis_work(bytes as u64 + 1)?;
        self.environment
            .budget
            .reserve_temporary_bytes(bytes as u64)?;
        self.prepay_source_lookup(symbol)?;
        let source = self.source_declaration(symbol, span)?;
        let parameters = match &source.kind {
            s::ItemKind::Function(f) | s::ItemKind::Extern { function: f, .. } => &f.generics,
            s::ItemKind::Aggregate(a) => &a.generics,
            s::ItemKind::Enum(e) => &e.generics,
            s::ItemKind::Trait(t) => &t.generics,
            s::ItemKind::Impl(i) => &i.generics,
            _ => unreachable!("checked source header"),
        };
        Ok(parameters[ordinal as usize].name.clone())
    }

    pub(super) fn generated_symbol(
        &mut self,
        tree: &GeneratedSourceTree,
        local: zil_base::NodeId,
        span: Span,
    ) -> CompileResult<CtSymbolId> {
        let SourceTreeIdentity::Generated { module, event_path } = &tree.identity else {
            return Err(category(
                "generated symbol has an original source tree",
                span,
            ));
        };
        let declaration = *tree
            .declarations
            .get(local.0 as usize)
            .ok_or_else(|| category("generated symbol source declaration absent", span))?;
        self.environment
            .budget
            .reserve_analysis_work(event_path.len() as u64 + 1)?;
        Ok(CtSymbolId {
            module: *module,
            declaration,
            generation_path: tree.generation_fingerprint(),
        })
    }

    pub(super) fn freeze_generated_items(
        &mut self,
        syntax: &GeneratedSyntax,
        module: ModuleId,
        span: Span,
    ) -> CompileResult<()> {
        let GeneratedSyntax::ItemList(items) = syntax else {
            return Ok(());
        };
        let Some(emitter) = &self.current_emitter else {
            return Err(category(
                "generated ItemList has no structural emitter",
                span,
            ));
        };
        let parent_len = match &emitter.tree {
            SourceTreeIdentity::Original(_) => 0,
            SourceTreeIdentity::Generated { event_path, .. } => event_path.len(),
        };
        let path_payload = (parent_len * 2 + emitter.event_path.len() * 2 + 1) as u64 * 4;
        self.environment
            .budget
            .reserve_analysis_work(path_payload / 4 + 1)?;
        self.environment
            .budget
            .reserve_temporary_bytes(path_payload)?;
        let mut event_path = Vec::with_capacity(parent_len + emitter.event_path.len() + 1);
        match &emitter.tree {
            SourceTreeIdentity::Original(_) => event_path.push(0),
            SourceTreeIdentity::Generated {
                event_path: parent, ..
            } => {
                event_path.extend_from_slice(parent);
                event_path.push(1);
            }
        }
        event_path.extend_from_slice(&emitter.event_path);
        self.environment
            .budget
            .reserve_analysis_work(event_path.len() as u64 + 1)?;
        let mut count = items.len();
        for item in items {
            count = count
                .checked_add(children_count(
                    item,
                    0,
                    self.max_depth,
                    &mut self.environment.budget,
                )?)
                .ok_or_else(|| category("generated declaration count overflow", span))?;
        }
        self.environment.budget.reserve_analysis_work(
            1 + self
                .next_generated_declaration
                .len()
                .checked_ilog2()
                .unwrap_or(0) as u64,
        )?;
        if !self.next_generated_declaration.contains_key(&module) {
            self.environment
                .budget
                .reserve_analysis_work(self.parsed.len() as u64)?;
            let source = self
                .parsed
                .iter()
                .find(|p| p.module.id == module)
                .ok_or_else(|| category("generated source module absent", span))?;
            let mut count = source.module.items.len();
            for item in &source.module.items {
                count = count
                    .checked_add(children_count(
                        item,
                        0,
                        self.max_depth,
                        &mut self.environment.budget,
                    )?)
                    .ok_or_else(|| category("original declaration count overflow", span))?;
            }
            self.environment.budget.reserve_temporary_bytes(64)?;
            self.next_generated_declaration.insert(
                module,
                u32::try_from(count)
                    .map_err(|_| category("original declaration IDs exhausted", span))?,
            );
        }
        let next = self.next_generated_declaration.get_mut(&module).unwrap();
        let end = next
            .checked_add(
                u32::try_from(count)
                    .map_err(|_| category("generated declaration IDs exhausted", span))?,
            )
            .ok_or_else(|| category("generated declaration IDs exhausted", span))?;
        self.environment
            .budget
            .reserve_generated_nodes(count as u64 + 1)?;
        self.environment.budget.reserve_temporary_bytes(
            (count * std::mem::size_of::<zil_base::NodeId>()) as u64
                + (std::mem::size_of::<GeneratedSourceTree>() + std::mem::size_of::<s::Module>())
                    as u64
                + event_path.len() as u64 * 4,
        )?;
        // The ordinary bounded syntax walk prepays the entire retained clone.
        // Its per-node envelope includes every AST node/header alternative;
        // text work accounts for all string allocation payloads.
        for item in items {
            let cost = zil_expand::ct_selection::item_cost(
                item,
                &self.environment.remaining_limits_for_input(),
            )?;
            self.environment
                .budget
                .reserve_analysis_work(cost.work as u64)?;
            self.environment
                .budget
                .reserve_generated_nodes(cost.nodes as u64)?;
            self.environment.budget.reserve_temporary_bytes(
                (cost.nodes as u64)
                    .saturating_mul(
                        (std::mem::size_of::<s::Item>()
                            + std::mem::size_of::<s::Stmt>()
                            + std::mem::size_of::<s::Expr>()
                            + std::mem::size_of::<s::TypeRef>()
                            + std::mem::size_of::<s::Pattern>()) as u64,
                    )
                    .saturating_add(cost.work as u64),
            )?;
        }
        let tree = Arc::new(GeneratedSourceTree {
            identity: SourceTreeIdentity::Generated { module, event_path },
            emitter: emitter.clone(),
            module: Arc::new(s::Module {
                id: module,
                items: items.clone(),
                span,
            }),
            declarations: (*next..end).map(zil_base::NodeId).collect(),
        });
        *next = end;
        self.environment.budget.reserve_analysis_work(
            (self.generated_trees.len() as u64 + 1).saturating_mul(tree.identity_path_len() as u64),
        )?;
        if self
            .generated_trees
            .iter()
            .any(|old| old.identity == tree.identity)
        {
            return Err(category("generated source event repeats", span));
        }
        if self.generated_trees.len() == self.generated_trees.capacity() {
            let old_bytes = self.generated_trees.capacity() as u64
                * std::mem::size_of::<Arc<GeneratedSourceTree>>() as u64;
            self.environment.budget.reserve_temporary_bytes(
                old_bytes + std::mem::size_of::<Arc<GeneratedSourceTree>>() as u64,
            )?;
            self.generated_trees.reserve_exact(1);
            self.environment.budget.release_temporary_bytes(old_bytes);
        }
        self.environment.register_generated_tree(tree.clone())?;
        self.generated_trees.push(tree.clone());
        self.pending_generated_tree = Some(tree);
        Ok(())
    }

    pub(super) fn generated_origin(
        &mut self,
        tree: &Arc<GeneratedSourceTree>,
        root: usize,
    ) -> CompileResult<DeclarationOrigin> {
        let span = tree.module.span;
        let _original = tree
            .module
            .items
            .get(root)
            .ok_or_else(|| category("generated source root absent", span))?;
        let declaration = *tree
            .declarations
            .get(root)
            .ok_or_else(|| category("generated source declaration ID absent", span))?;
        let SourceTreeIdentity::Generated { event_path, .. } = &tree.identity else {
            return Err(category("generated source tree is original", span));
        };
        // Prepay all namespace/path payloads before cloning the root identity.
        let template = self
            .source_namespaces
            .get(&tree.module.id)
            .ok_or_else(|| category("generated declaration source namespace absent", span))?;
        let namespace_bytes = super::super::invocation_context::identity_cost(
            template,
            &mut self.environment.budget,
        )?;
        self.environment.budget.reserve_temporary_bytes(
            namespace_bytes
                + std::mem::size_of::<DeclarationOrigin>() as u64
                + 2 * std::mem::size_of::<String>() as u64
                + 32
                + event_path.len() as u64 * 4,
        )?;
        let identity = tree
            .root_identity(root as u32, template)
            .ok_or_else(|| category("generated source root identity absent", span))?;
        let retained_members = self.take_generated_member_retention(tree, declaration)?;
        Ok(DeclarationOrigin {
            closure_sites: ClosureSiteLineage::Unavailable,
            declaration,
            identity,
            retained_members,
            generated: Some(GeneratedDeclarationOrigin {
                tree: tree.clone(),
                root: root as u32,
            }),
        })
    }
}
