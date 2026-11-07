//! Private declaration provenance assigned by actual module traversal. Spans
//! and generated spellings are never owner authority.
use super::*;
use crate::source_context::{
    DeclarationOrigin, DeclarationPlacement, ResolvedSourceIdentityContext,
};
use zil_base::ModuleId;
use zil_base::NodeId;
use zil_ir::LifeDeclarationIdentity;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct DeclId {
    pub(crate) module: ModuleId,
    pub(crate) ordinal: u32,
    pub(crate) compiler: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct MemberId {
    pub(crate) declaration: DeclId,
    pub(crate) ordinal: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Origin {
    pub(crate) declaration: DeclId,
    pub(crate) member: Option<u32>,
    pub(crate) provider: bool,
    pub(crate) compiler: bool,
}
#[derive(Default)]
pub(crate) struct Origins {
    pub(crate) entries: Vec<Origin>,
    pub(crate) identities: BTreeMap<DeclId, DeclarationOrigin>,
    /// Actual output placement after project redistribution. The source
    /// declaration identity remains in Origin and is never a final vector index.
    pub(crate) modules: BTreeMap<ModuleId, Vec<Origin>>,
}
impl Origin {
    pub(crate) fn source(module: ModuleId, ordinal: usize, span: Span) -> R<Self> {
        Ok(Self {
            declaration: DeclId {
                module,
                compiler: false,
                ordinal: u32::try_from(ordinal).map_err(|_| {
                    Diagnostic::limit(
                        Phase::Expand,
                        "declaration identity ordinal exhausted",
                        Some(span),
                    )
                })?,
            },
            member: None,
            provider: false,
            compiler: false,
        })
    }
    pub(crate) fn member(self, ordinal: usize, span: Span) -> R<Self> {
        Ok(Self {
            member: Some(u32::try_from(ordinal).map_err(|_| {
                Diagnostic::limit(
                    Phase::Expand,
                    "member identity ordinal exhausted",
                    Some(span),
                )
            })?),
            provider: false,
            ..self
        })
    }
    pub(crate) fn id(self, span: Span) -> R<MemberId> {
        Ok(MemberId {
            declaration: self.declaration,
            ordinal: self
                .member
                .ok_or_else(|| error(span, "method source member identity absent"))?,
        })
    }
}
impl Origins {
    pub(crate) fn aggregate_member(
        &self,
        parent: Origin,
        output: usize,
        budget: &mut Budget<'_>,
        span: Span,
    ) -> R<Origin> {
        let ordinal = if self.identities.is_empty() {
            output
        } else {
            let captured = self.identity(parent.declaration, budget, span)?;
            budget.identity_work(1, span)?;
            if let Some(retained) = &captured.retained_members {
                *retained
                    .positions
                    .get(output)
                    .ok_or_else(|| error(span, "retained member output ordinal is absent"))?
                    as usize
            } else {
                output
            }
        };
        parent.member(ordinal, span)
    }

    pub(crate) fn single(
        module: &Module,
        inserted_option: bool,
        budget: &mut Budget<'_>,
    ) -> R<Self> {
        let mut entries = Vec::new();
        for (index, item) in module.items.iter().enumerate() {
            budget.node(item.span, 0)?;
            let mut origin = Origin::source(
                module.id,
                index.saturating_sub(usize::from(inserted_option)),
                item.span,
            )?;
            if inserted_option && index == 0 {
                origin.compiler = true;
                origin.declaration.compiler = true;
            }
            entries.push(origin);
        }
        Ok(Self {
            entries,
            identities: BTreeMap::new(),
            modules: BTreeMap::new(),
        })
    }
    pub(crate) fn insert_option(&mut self, module: &Module, budget: &mut Budget<'_>) -> R<()> {
        if self.entries.len() + 1 != module.items.len() {
            return Err(error(module.span, "Option provenance shape differs"));
        }
        for _ in 0..=self.entries.len() {
            budget.node(module.span, 0)?;
        }
        let mut origin = Origin::source(module.id, 0, module.span)?;
        origin.compiler = true;
        origin.declaration.compiler = true;
        self.entries.insert(0, origin);
        Ok(())
    }
    pub(crate) fn aliases(&mut self, module: &Module, budget: &mut Budget<'_>) -> R<()> {
        if self.entries.len() != module.items.len() {
            return Err(error(module.span, "alias provenance shape differs"));
        }
        let mut retained = Vec::new();
        for (item, origin) in module.items.iter().zip(std::mem::take(&mut self.entries)) {
            budget.node(item.span, 0)?;
            if !matches!(item.kind, ItemKind::TypeAlias(_)) {
                retained.push(origin);
            }
        }
        self.entries = retained;
        Ok(())
    }
    pub(crate) fn get(&self, index: usize, span: Span) -> R<Origin> {
        self.entries
            .get(index)
            .copied()
            .ok_or_else(|| error(span, "declaration provenance absent"))
    }
    pub(crate) fn in_module(
        &self,
        module: ModuleId,
        index: usize,
        budget: &mut Budget<'_>,
        span: Span,
    ) -> R<Origin> {
        for _ in 0..self
            .modules
            .len()
            .checked_ilog2()
            .map_or(1, |n| n as usize + 2)
        {
            budget.node(span, 0)?;
        }
        let origin = if self.modules.is_empty() {
            self.get(index, span)?
        } else {
            *self
                .modules
                .get(&module)
                .and_then(|entries| entries.get(index))
                .ok_or_else(|| error(span, "output module declaration origin absent"))?
        };
        if origin.declaration.module != module {
            return Err(error(span, "output declaration owner differs"));
        }
        Ok(origin)
    }
    pub(crate) fn identity(
        &self,
        declaration: DeclId,
        budget: &mut Budget<'_>,
        span: Span,
    ) -> R<&DeclarationOrigin> {
        for _ in 0..self
            .identities
            .len()
            .checked_ilog2()
            .map_or(1, |n| n as usize + 2)
        {
            budget.node(span, 0)?;
        }
        self.identities
            .get(&declaration)
            .ok_or_else(|| error(span, "original declaration identity absent"))
    }

    pub(crate) fn capture_identities(
        &mut self,
        module: &Module,
        inserted_option: bool,
        context: &ResolvedSourceIdentityContext<'_>,
        budget: &mut Budget<'_>,
    ) -> R<()> {
        if self.entries.len() != module.items.len() {
            return Err(error(
                module.span,
                "identity provenance is not aligned with syntax",
            ));
        }
        let mut supplied = None;
        match context.declarations() {
            DeclarationPlacement::Unavailable => {
                return Err(unsupported(
                    module.span,
                    "transformed declarations require explicit source lineage",
                ));
            }
            DeclarationPlacement::Original => {}
            DeclarationPlacement::Aligned(modules) => {
                for candidate in modules {
                    budget.node(module.span, 0)?;
                    if candidate.module == module.id {
                        if supplied.is_some() {
                            return Err(error(module.span, "duplicate declaration-origin module"));
                        }
                        supplied = Some(candidate);
                    }
                }
                let actual = supplied
                    .ok_or_else(|| error(module.span, "declaration-origin module absent"))?;
                if actual.items.len().checked_add(usize::from(inserted_option))
                    != Some(module.items.len())
                {
                    return Err(error(module.span, "declaration-origin item count differs"));
                }
            }
        }
        let module_path = context.module_path(module.id, budget, module.span)?;
        if context.package.is_empty() {
            return Err(error(module.span, "source package namespace is empty"));
        }
        let mut seen_ids = BTreeSet::new();
        let mut seen_identities = BTreeSet::new();
        let mut seen_generated_trees = BTreeMap::new();
        let mut seen_generated_ids = BTreeSet::new();
        for (index, (item, origin)) in module.items.iter().zip(&self.entries).enumerate() {
            budget.node(item.span, 0)?;
            if origin.declaration.module != module.id {
                continue;
            }
            if origin.declaration.compiler {
                continue;
            }
            let ordinal = index
                .checked_sub(usize::from(inserted_option))
                .ok_or_else(|| error(item.span, "source declaration ordinal underflow"))?;
            let captured = if let Some(supplied) = supplied {
                let captured = &supplied.items[ordinal];
                pay_identity(&captured.identity, budget, item.span)?;
                validate_retained_members(item, captured, budget)?;
                budget.text(&context.package, item.span)?;
                if captured.identity.package != context.package {
                    return Err(error(
                        item.span,
                        "declaration package differs from request context",
                    ));
                }
                for part in module_path {
                    budget.node(item.span, 0)?;
                    budget.text(part, item.span)?;
                }
                if captured.identity.module_path != module_path {
                    return Err(error(
                        item.span,
                        "aligned declaration canonical module differs",
                    ));
                }
                let (kind, name) = declaration_shape(item);
                budget.text(kind, item.span)?;
                let path = &captured.identity.declaration_path;
                if path.len() != 2 || path[0] != kind {
                    return Err(error(
                        item.span,
                        "aligned original declaration kind differs",
                    ));
                }
                if let Some(generated) = &captured.generated {
                    validate_generated(item, captured, module.id, budget)?;
                    let tree = &generated.tree;
                    let identity_work = tree.identity_path_len();
                    budget.identity_work(
                        identity_work.saturating_mul(
                            seen_generated_trees.len().checked_ilog2().unwrap_or(0) as usize + 1,
                        ),
                        item.span,
                    )?;
                    if let Some(previous) = seen_generated_trees.get(&tree.identity) {
                        if !std::sync::Arc::ptr_eq(previous, tree) {
                            return Err(error(
                                item.span,
                                "one generated event refers to different frozen trees",
                            ));
                        }
                    } else {
                        budget.identity_work(
                            tree.declarations.len().saturating_mul(
                                supplied
                                    .items
                                    .len()
                                    .saturating_add(seen_generated_ids.len())
                                    .saturating_add(1),
                            ),
                            item.span,
                        )?;
                        budget.lineage_bytes(
                            96usize.saturating_add(tree.declarations.len().saturating_mul(64)),
                            item.span,
                        )?;
                        for declaration in &tree.declarations {
                            if supplied.items.iter().any(|row| {
                                row.generated.is_none() && row.declaration == *declaration
                            }) || !seen_generated_ids.insert(*declaration)
                            {
                                return Err(error(
                                    item.span,
                                    "generated declaration inventory collides across source trees",
                                ));
                            }
                        }
                        seen_generated_trees.insert(&tree.identity, tree.clone());
                    }
                } else if !captured.identity.generation_path.is_empty() {
                    return Err(error(
                        item.span,
                        "generated identity has no actual source ledger",
                    ));
                } else if let Some(name) = name {
                    budget.text(name, item.span)?;
                    if path[1] != name {
                        return Err(error(
                            item.span,
                            "aligned original declaration binding differs",
                        ));
                    }
                }
                for _ in 0..12usize
                    .saturating_mul(seen_ids.len().checked_ilog2().map_or(1, |n| n as usize + 2))
                {
                    budget.node(item.span, 0)?;
                }
                if !seen_ids.insert(captured.declaration) {
                    return Err(error(item.span, "duplicate aligned source declaration"));
                }
                for _ in 0..12usize.saturating_mul(
                    seen_identities
                        .len()
                        .checked_ilog2()
                        .map_or(1, |n| n as usize + 2),
                ) {
                    pay_identity(&captured.identity, budget, item.span)?;
                }
                if !seen_identities.insert(&captured.identity) {
                    return Err(error(item.span, "duplicate aligned declaration identity"));
                }
                pay_declaration_copy(captured, budget, item.span)?;
                captured.clone()
            } else {
                let (kind, actual_name) = declaration_shape(item);
                budget.text(&context.package, item.span)?;
                for part in module_path {
                    budget.node(item.span, 0)?;
                    budget.text(part, item.span)?;
                }
                budget.text(kind, item.span)?;
                let actual_name = if let Some(actual_name) = actual_name {
                    budget.text(actual_name, item.span)?;
                    actual_name.to_owned()
                } else {
                    // An anonymous declaration uses its actual source structural
                    // ordinal. This is not a location/span or a generated name.
                    for _ in 0..=ordinal.checked_ilog10().unwrap_or(0) {
                        budget.node(item.span, 0)?;
                    }
                    ordinal.to_string()
                };
                budget.lineage_bytes(std::mem::size_of::<DeclarationOrigin>(), item.span)?;
                DeclarationOrigin {
                    generated: None,
                    closure_sites: crate::source_context::ClosureSiteLineage::Original,
                    retained_members: None,
                    declaration: NodeId(
                        u32::try_from(ordinal).map_err(|_| {
                            error(item.span, "source declaration identity exhausted")
                        })?,
                    ),
                    identity: LifeDeclarationIdentity {
                        package: context.package.to_owned(),
                        module_path: module_path.to_vec(),
                        declaration_path: vec![kind.to_owned(), actual_name],
                        generation_path: Vec::new(),
                    },
                }
            };
            for _ in 0..self
                .identities
                .len()
                .checked_ilog2()
                .map_or(1, |n| n as usize + 2)
            {
                budget.node(item.span, 0)?;
            }
            self.identities.insert(origin.declaration, captured);
        }
        Ok(())
    }

    pub(crate) fn copy_identities(
        &self,
        budget: &mut Budget<'_>,
        span: Span,
    ) -> R<BTreeMap<DeclId, DeclarationOrigin>> {
        for identity in self.identities.values() {
            budget.node(span, 0)?;
            pay_identity(&identity.identity, budget, span)?;
            pay_declaration_copy(identity, budget, span)?;
        }
        Ok(self.identities.clone())
    }
}

fn validate_generated(
    item: &Item,
    row: &DeclarationOrigin,
    module: ModuleId,
    budget: &mut Budget<'_>,
) -> R<()> {
    use crate::source_context::SourceTreeIdentity;
    let generated = row
        .generated
        .as_ref()
        .ok_or_else(|| error(item.span, "generated source ledger absent"))?;
    let tree = &generated.tree;
    let SourceTreeIdentity::Generated {
        module: owner,
        event_path,
    } = &tree.identity
    else {
        return Err(error(
            item.span,
            "generated ledger has original tree identity",
        ));
    };
    budget.identity_work(
        event_path
            .len()
            .saturating_add(tree.declarations.len())
            .saturating_add(1),
        item.span,
    )?;
    if *owner != module
        || tree.module.id != module
        || tree.emitter.tree.module() != module
        || event_path.is_empty()
        || tree.emitter.event_path.is_empty()
        || row.identity.generation_path != *event_path
        || tree.declarations.get(generated.root as usize) != Some(&row.declaration)
        || tree
            .declarations
            .windows(2)
            .any(|pair| pair[0].0.checked_add(1) != Some(pair[1].0))
    {
        return Err(error(
            item.span,
            "generated tree, event or unique declaration inventory differs",
        ));
    }
    let event_matches = match &tree.emitter.tree {
        SourceTreeIdentity::Original(_) => {
            event_path.first() == Some(&0)
                && event_path.get(1..) == Some(tree.emitter.event_path.as_slice())
        }
        SourceTreeIdentity::Generated {
            event_path: parent, ..
        } => {
            budget.identity_work(parent.len(), item.span)?;
            event_path.starts_with(parent)
                && event_path.get(parent.len()) == Some(&1)
                && event_path.get(parent.len() + 1..) == Some(tree.emitter.event_path.as_slice())
        }
    };
    if !event_matches {
        return Err(error(
            item.span,
            "generated emitter structural path differs",
        ));
    }
    let mut inventory_count = tree.module.items.len();
    for source in &tree.module.items {
        inventory_count = inventory_count
            .checked_add(generated_children_count(source, budget, 0)?)
            .ok_or_else(|| error(item.span, "generated source inventory count overflow"))?;
    }
    if tree.declarations.len() != inventory_count {
        return Err(error(
            item.span,
            "generated source inventory does not cover actual declarations",
        ));
    }
    let original = tree
        .module
        .items
        .get(generated.root as usize)
        .ok_or_else(|| error(item.span, "generated ledger root absent"))?;
    let (kind, _) = declaration_shape(original);
    budget.text(kind, item.span)?;
    budget.lineage_bytes(10, item.span)?;
    let root_name = generated.root.to_string();
    budget.text(&root_name, item.span)?;
    if row.identity.declaration_path.len() != 2
        || row.identity.declaration_path[0] != kind
        || row.identity.declaration_path[1] != root_name
    {
        return Err(error(
            item.span,
            "generated declaration structural root differs",
        ));
    }
    // Reproduce diagnostic remapping only, then compare the actual declaration
    // header and complete generic binder sequence. CT body rewrites are not
    // declaration identity and may legitimately differ after materialization.
    let limits = budget.remaining_limits();
    let cost =
        crate::ct_selection::item_cost(original, &limits).map_err(|mut errors| errors.remove(0))?;
    budget.identity_work(cost.work, item.span)?;
    budget.lineage_bytes(
        cost.nodes
            .saturating_mul(
                std::mem::size_of::<Item>()
                    + std::mem::size_of::<Stmt>()
                    + std::mem::size_of::<Expr>()
                    + std::mem::size_of::<TypeRef>()
                    + std::mem::size_of::<Pattern>(),
            )
            .saturating_add(cost.work),
        item.span,
    )?;
    let mut syntax = zil_syntax::GeneratedSyntax::ItemList(vec![original.clone()]);
    let work = crate::remap_generated_spans(&mut syntax, item.span, &budget.remaining_limits())
        .map_err(|mut errors| errors.remove(0))?;
    budget.identity_work(work, item.span)?;
    let zil_syntax::GeneratedSyntax::ItemList(original) = syntax else {
        unreachable!()
    };
    let original = &original[0];
    // Deferred runtime CT retains the entire generated function, unlike other
    // materialized declarations whose bodies may legitimately change. Compare
    // every semantic AST node before module normalization or specialization.
    // ClosureSiteLineage remains Unavailable: this is a separate obligation.
    if let ItemKind::Function(function) = &original.kind
        && function
            .generics
            .iter()
            .any(|parameter| !matches!(parameter.kind, GenericParamKind::Lifetime))
    {
        struct Deferred<'a, 'b> {
            budget: &'a mut Budget<'b>,
            found: bool,
        }
        impl crate::ct_selection::Visitor for Deferred<'_, '_> {
            fn node(&mut self, span: Span, depth: usize) -> CompileResult<()> {
                self.budget.node(span, depth).map_err(|error| vec![error])
            }
            fn text(&mut self, text: &str, span: Span) -> CompileResult<()> {
                self.budget.text(text, span).map_err(|error| vec![error])
            }
            fn expression(&mut self, expression: &Expr, depth: usize) -> CompileResult<()> {
                self.found |= matches!(expression.kind, ExprKind::MetaEval(_));
                crate::ct_selection::walk_expression(self, expression, depth)
            }
        }
        let mut scan = Deferred {
            budget,
            found: false,
        };
        match &function.body {
            FunctionBody::Block(block) => crate::ct_selection::Visitor::block(&mut scan, block, 0),
            FunctionBody::Expression(expression) => {
                crate::ct_selection::Visitor::expression(&mut scan, expression, 0)
            }
            FunctionBody::Declaration => Ok(()),
        }
        .map_err(|mut errors| errors.remove(0))?;
        if scan.found {
            let cost = crate::ct_selection::item_cost(item, &budget.remaining_limits())
                .map_err(|mut errors| errors.remove(0))?;
            budget.identity_work(cost.work, item.span)?;
            if original != item {
                return Err(error(
                    item.span,
                    "generated deferred runtime function differs from its complete frozen source body",
                ));
            }
        }
    }
    fn binders(item: &Item) -> &[GenericParam] {
        match &item.kind {
            ItemKind::Function(f) | ItemKind::Extern { function: f, .. } => f.generics.as_slice(),
            ItemKind::Aggregate(a) => a.generics.as_slice(),
            ItemKind::Enum(e) => e.generics.as_slice(),
            ItemKind::Trait(t) => t.generics.as_slice(),
            ItemKind::Impl(i) => i.generics.as_slice(),
            ItemKind::TypeAlias(a) => a.generics.as_slice(),
            _ => &[],
        }
    }
    fn headers(
        original: &Item,
        item: &Item,
        retained: Option<&crate::source_context::RetainedMemberOrigins>,
        budget: &mut Budget<'_>,
        depth: usize,
    ) -> R<()> {
        budget.node(item.span, depth)?;
        budget.identity_work(
            binders(original).len().saturating_add(binders(item).len()),
            item.span,
        )?;
        if declaration_shape(original) != declaration_shape(item)
            || original.public != item.public
            || original.attributes != item.attributes
            || binders(original) != binders(item)
        {
            return Err(error(
                item.span,
                "generated output declaration header or full binder order differs",
            ));
        }
        let (source, output) = match (&original.kind, &item.kind) {
            (ItemKind::Aggregate(a), ItemKind::Aggregate(b)) => {
                (a.methods.as_slice(), b.methods.as_slice())
            }
            (ItemKind::Trait(a), ItemKind::Trait(b)) => {
                (a.members.as_slice(), b.members.as_slice())
            }
            (ItemKind::Impl(a), ItemKind::Impl(b)) => (a.members.as_slice(), b.members.as_slice()),
            _ => return Ok(()),
        };
        if retained.is_none() && source.len() != output.len() {
            return Err(error(
                item.span,
                "generated members changed without retained source events",
            ));
        }
        if let Some(retained) = retained
            && (retained.original_count as usize != source.len()
                || retained.positions.len() != output.len())
        {
            return Err(error(
                item.span,
                "generated retained member source count differs",
            ));
        }
        for (ordinal, member) in output.iter().enumerate() {
            let source_ordinal = retained.map_or(ordinal, |map| map.positions[ordinal] as usize);
            let original = source.get(source_ordinal).ok_or_else(|| {
                error(item.span, "generated retained member source ordinal absent")
            })?;
            headers(original, member, None, budget, depth + 1)?;
        }
        Ok(())
    }
    headers(original, item, row.retained_members.as_ref(), budget, 0)
}

fn generated_children_count(item: &Item, budget: &mut Budget<'_>, depth: usize) -> R<usize> {
    budget.node(item.span, depth)?;
    let (mut count, members) = match &item.kind {
        ItemKind::Aggregate(host) => (host.fields.len(), host.methods.as_slice()),
        ItemKind::Trait(host) => (0, host.members.as_slice()),
        ItemKind::Impl(host) => (0, host.members.as_slice()),
        ItemKind::Enum(host) => {
            let mut count = host.variants.len();
            for variant in &host.variants {
                budget.node(variant.span, depth + 1)?;
                count += match &variant.fields {
                    VariantFields::Unit => 0,
                    VariantFields::Tuple(fields) => fields.len(),
                    VariantFields::Record(fields) => fields.len(),
                };
            }
            (count, &[][..])
        }
        _ => (0, &[][..]),
    };
    budget.identity_work(count.saturating_add(members.len()), item.span)?;
    for member in members {
        count = count
            .checked_add(1)
            .and_then(|n| n.checked_add(0))
            .ok_or_else(|| error(item.span, "generated source inventory overflow"))?;
        count = count
            .checked_add(generated_children_count(member, budget, depth + 1)?)
            .ok_or_else(|| error(item.span, "generated source inventory overflow"))?;
    }
    Ok(count)
}

pub(crate) fn pay_declaration_copy(
    row: &DeclarationOrigin,
    budget: &mut Budget<'_>,
    span: Span,
) -> R<()> {
    budget.lineage_bytes(std::mem::size_of::<DeclarationOrigin>(), span)?;
    if let Some(members) = &row.retained_members {
        budget.identity_work(members.positions.len().saturating_add(1), span)?;
        budget.lineage_bytes(
            members
                .positions
                .len()
                .saturating_mul(std::mem::size_of::<u32>()),
            span,
        )?;
    }
    Ok(())
}

fn validate_retained_members(
    item: &Item,
    row: &DeclarationOrigin,
    budget: &mut Budget<'_>,
) -> R<()> {
    let Some(members) = &row.retained_members else {
        return Ok(());
    };
    let ItemKind::Aggregate(host) = &item.kind else {
        return Err(error(
            item.span,
            "retained members require a nongeneric original struct",
        ));
    };
    budget.identity_work(members.positions.len().saturating_add(1), item.span)?;
    if host.kind != AggregateKind::Struct
        || (!row.identity.generation_path.is_empty() && row.generated.is_none())
        || members.positions.len() != host.methods.len()
        || members.original_count as usize <= members.positions.len()
        || members
            .positions
            .iter()
            .any(|position| *position >= members.original_count)
        || members.positions.windows(2).any(|pair| pair[0] >= pair[1])
    {
        return Err(error(
            item.span,
            "retained member table is not a complete deletion-only struct correspondence",
        ));
    }
    Ok(())
}

pub(crate) fn pay_identity(
    identity: &LifeDeclarationIdentity,
    budget: &mut Budget<'_>,
    span: Span,
) -> R<()> {
    budget.text(&identity.package, span)?;
    for part in identity
        .module_path
        .iter()
        .chain(&identity.declaration_path)
    {
        budget.node(span, 0)?;
        budget.text(part, span)?;
    }
    for _ in &identity.generation_path {
        budget.node(span, 0)?;
    }
    Ok(())
}
pub(crate) struct Pending {
    pub(crate) item: Item,
    pub(crate) origin: Option<Origin>,
    pub(crate) source_enum_return: Option<TypeRef>,
    pub(crate) member_calls: BTreeMap<usize, trait_instances::CallTargets>,
    pub(crate) runtime_calls: Option<BTreeMap<usize, u32>>,
}
impl Engine<'_> {
    pub(crate) fn enqueue(&mut self, item: Item, origin: Option<Origin>) -> R<()> {
        if self.origins.is_some() {
            self.budget.node(item.span, 0)?;
            if origin.is_none() {
                return Err(error(item.span, "generated declaration provenance absent"));
            }
        }
        self.pending.push_back(Pending {
            item,
            origin,
            source_enum_return: None,
            member_calls: BTreeMap::new(),
            runtime_calls: None,
        });
        Ok(())
    }
    pub(crate) fn template_origin(&mut self, item: &Item) -> R<Option<Origin>> {
        self.traits.origin(item, &mut self.budget)
    }
}

#[derive(Default, Clone, Copy)]
pub(crate) struct WalkContext {
    pub(crate) origin: Option<Origin>,
}
impl Budget<'_> {
    pub(crate) fn with_origin<T>(
        &mut self,
        origin: Option<Origin>,
        f: impl FnOnce(&mut Self) -> R<T>,
    ) -> R<T> {
        let previous = std::mem::replace(&mut self.context.origin, origin);
        let result = f(self);
        self.context.origin = previous;
        result
    }
}
impl Engine<'_> {
    pub(super) fn with_origin<T>(
        &mut self,
        origin: Option<Origin>,
        f: impl FnOnce(&mut Self) -> R<T>,
    ) -> R<T> {
        let previous = std::mem::replace(&mut self.current_origin, origin);
        let budget_previous = std::mem::replace(&mut self.budget.context.origin, origin);
        let result = f(self);
        self.current_origin = previous;
        self.budget.context.origin = budget_previous;
        result
    }
    pub(super) fn provider(&mut self, name: &str, span: Span) -> R<bool> {
        if self.origins.is_none() {
            return Ok(false);
        }
        let Some(item) = self.templates.get(name).copied() else {
            return Ok(false);
        };
        self.budget.node(span, 0)?;
        Ok(self
            .traits
            .origin(item, &mut self.budget)?
            .is_some_and(|origin| origin.provider))
    }
}

fn declaration_shape(item: &Item) -> (&'static str, Option<&str>) {
    match &item.kind {
        ItemKind::Function(f) => ("function", Some(f.name.as_str())),
        ItemKind::Extern { function, .. } => ("extern", Some(function.name.as_str())),
        ItemKind::Aggregate(a) => (
            match a.kind {
                AggregateKind::Struct => "struct",
                AggregateKind::Package => "package",
                AggregateKind::Resource => "resource",
            },
            Some(a.name.as_str()),
        ),
        ItemKind::Enum(e) => ("enum", Some(e.name.as_str())),
        ItemKind::Trait(t) => ("trait", Some(t.name.as_str())),
        ItemKind::TypeAlias(a) => ("alias", Some(a.name.as_str())),
        ItemKind::Const(c) => ("const", Some(c.name.as_str())),
        ItemKind::Impl(_) => ("impl", None),
        ItemKind::Import(_) => ("import", None),
        ItemKind::Meta(_) => ("meta", None),
    }
}

#[cfg(test)]
mod retained_member_tests {
    use super::*;
    use crate::source_context::{RetainedMemberOrigins, SourceIdentityContext, capture_original};
    use zil_base::{FileId, Limits, SourceUnit};

    #[test]
    fn aligned_member_shapes_and_source_lookup_are_bounded() {
        let source = SourceUnit::new(
            FileId(0),
            ModuleId(0),
            "member-shapes.zil",
            "fn erased()->Type{return i32}struct Tag<life a>{n:i32}struct Maker{fn gone()->i32{return 0}fn left()->i32{return 7}fn right()->i32{return 35}}fn main()->i32{return 0}",
        );
        let parsed = zil_syntax::parse(&source);
        assert!(parsed.diagnostics.is_empty());
        let paths = BTreeMap::new();
        let limits = Limits::default();
        let (rows, _) = capture_original(
            std::slice::from_ref(&source),
            &[&parsed.module],
            SourceIdentityContext::ClosedSnapshot {
                root: ModuleId(0),
                aliases: &paths,
                declarations: DeclarationPlacement::Original,
            },
            &limits,
        )
        .unwrap();
        let mut rows = rows.unwrap();
        let mut actual = parsed.module.clone();
        let ItemKind::Aggregate(host) = &mut actual.items[2].kind else {
            unreachable!()
        };
        host.methods.remove(0);
        actual.items.remove(0);
        rows[0].items.remove(0);
        rows[0].items[1].retained_members = Some(RetainedMemberOrigins {
            original_count: 3,
            positions: vec![1, 2],
        });
        let load = |actual: &Module, rows: &[crate::source_context::ModuleDeclarationOrigins]| {
            let mut budget = Budget::new(&limits);
            let context = SourceIdentityContext::ClosedSnapshot {
                root: ModuleId(0),
                aliases: &paths,
                declarations: DeclarationPlacement::Aligned(rows),
            };
            let resolved = context.resolve(std::slice::from_ref(&source), &mut budget)?;
            let mut origins = Origins::single(actual, false, &mut budget)?;
            origins.capture_identities(actual, false, &resolved, &mut budget)?;
            Ok::<_, Diagnostic>(origins)
        };
        let origins = load(&actual, &rows).unwrap();
        let mut budget = Budget::new(&limits);
        let parent = origins.get(1, actual.span).unwrap();
        let first = origins
            .aggregate_member(parent, 0, &mut budget, actual.span)
            .unwrap();
        let second = origins
            .aggregate_member(parent, 1, &mut budget, actual.span)
            .unwrap();
        assert_eq!((first.member, second.member), (Some(1), Some(2)));
        // The table-placement ordinal is deliberately different from the
        // captured original host. Canonical identities use the latter.
        assert_eq!(parent.declaration.ordinal, 1);
        assert_eq!(
            origins
                .identity(parent.declaration, &mut budget, actual.span)
                .unwrap()
                .declaration
                .0,
            2
        );
        for positions in [vec![1], vec![1, 1], vec![2, 1], vec![1, 3], vec![0, 1, 2]] {
            let mut raw = rows.clone();
            raw[0].items[1].retained_members.as_mut().unwrap().positions = positions;
            assert!(load(&actual, &raw).is_err());
        }
        let mut generated = rows.clone();
        generated[0].items[1].identity.generation_path.push(0);
        assert!(load(&actual, &generated).is_err());
        let mut generic = actual.clone();
        let extra = zil_syntax::parse(&SourceUnit::new(
            FileId(0),
            ModuleId(0),
            "generic.zil",
            "struct Generic<X:Copy>{}",
        ));
        let ItemKind::Aggregate(template) = &extra.module.items[0].kind else {
            unreachable!()
        };
        let ItemKind::Aggregate(host) = &mut generic.items[1].kind else {
            unreachable!()
        };
        host.generics = template.generics.clone();
        // Generic hosts retain the same actual member ordinals; the table
        // alone grants no enclosing-instance or lifetime authority.
        assert!(load(&generic, &rows).is_ok());
        let mut resource = actual.clone();
        let ItemKind::Aggregate(host) = &mut resource.items[1].kind else {
            unreachable!()
        };
        host.kind = AggregateKind::Resource;
        assert!(load(&resource, &rows).is_err());
        for text in [
            "trait Other{fn a()->i32;}",
            "impl Other{fn a()->i32{return 0}}",
        ] {
            let parsed_kind = zil_syntax::parse(&SourceUnit::new(
                FileId(0),
                ModuleId(0),
                "unsupported.zil",
                text,
            ));
            assert!(parsed_kind.diagnostics.is_empty());
            let mut unsupported = actual.clone();
            unsupported.items[1].kind = parsed_kind.module.items[0].kind.clone();
            assert!(load(&unsupported, &rows).is_err());
        }
        // In-range false history is tested against immutable AST events in
        // Driver. This bare public Aligned reader proves structural validity.
    }
}

#[cfg(test)]
mod generated_lineage_tests {
    use super::*;
    use crate::source_context::{
        GeneratedDeclarationOrigin, GeneratedSourceTree, SourceDeclarationSite,
        SourceIdentityContext, SourceTreeIdentity, capture_original,
    };
    use std::sync::Arc;
    use zil_base::{FileId, Limits, SourceUnit};

    #[test]
    fn generated_raw_ledger_binds_actual_root_members_and_complete_binder_order() {
        let source = SourceUnit::new(
            FileId(0),
            ModuleId(0),
            "source.zil",
            "struct Anchor<life a>{n:i32}#emit quote<ItemList>{};fn main()->i32{return 0}",
        );
        let parsed = zil_syntax::parse(&source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let paths = BTreeMap::new();
        let limits = Limits::default();
        let (rows, _) = capture_original(
            std::slice::from_ref(&source),
            &[&parsed.module],
            SourceIdentityContext::ClosedSnapshot {
                root: ModuleId(0),
                aliases: &paths,
                declarations: DeclarationPlacement::Original,
            },
            &limits,
        )
        .unwrap();
        let mut rows = rows.unwrap();
        let emitted = zil_syntax::parse(&SourceUnit::new(
            FileId(9),
            ModuleId(0),
            "generated.zil",
            "struct Reader<life s>{fn result<T:Copy,#N:i32,life a>(self:Self,p:&'a T)->i32{return N}}",
        ));
        assert!(emitted.diagnostics.is_empty(), "{:?}", emitted.diagnostics);
        let tree = Arc::new(GeneratedSourceTree {
            identity: SourceTreeIdentity::Generated {
                module: ModuleId(0),
                event_path: vec![0, 1],
            },
            emitter: SourceDeclarationSite {
                tree: SourceTreeIdentity::Original(ModuleId(0)),
                declaration: NodeId(1),
                event_path: vec![1],
            },
            module: Arc::new(emitted.module),
            declarations: vec![NodeId(4), NodeId(5)],
        });
        let mut generated = zil_syntax::GeneratedSyntax::ItemList(tree.module.items.clone());
        crate::remap_generated_spans(&mut generated, parsed.module.items[1].span, &limits).unwrap();
        let zil_syntax::GeneratedSyntax::ItemList(mut generated) = generated else {
            unreachable!()
        };
        let mut output = parsed.module.clone();
        output.items[1] = generated.remove(0);
        rows[0].items[1] = DeclarationOrigin {
            closure_sites: crate::source_context::ClosureSiteLineage::Unavailable,
            declaration: NodeId(4),
            identity: tree.root_identity(0, &rows[0].items[0].identity).unwrap(),
            retained_members: None,
            generated: Some(GeneratedDeclarationOrigin {
                tree: tree.clone(),
                root: 0,
            }),
        };
        let load = |module: &Module, rows: &[crate::source_context::ModuleDeclarationOrigins]| {
            let mut budget = Budget::new(&limits);
            let context = SourceIdentityContext::ClosedSnapshot {
                root: ModuleId(0),
                aliases: &paths,
                declarations: DeclarationPlacement::Aligned(rows),
            }
            .resolve(std::slice::from_ref(&source), &mut budget)?;
            let mut origins = Origins::single(module, false, &mut budget)?;
            origins.capture_identities(module, false, &context, &mut budget)
        };
        load(&output, &rows).unwrap();
        let mut swapped = output.clone();
        let ItemKind::Aggregate(host) = &mut swapped.items[1].kind else {
            unreachable!()
        };
        let ItemKind::Function(member) = &mut host.methods[0].kind else {
            unreachable!()
        };
        member.generics.swap(0, 2);
        assert!(
            load(&swapped, &rows).is_err(),
            "member binder order must remain authentic"
        );
        for mutation in 0..4 {
            let mut forged = rows.clone();
            let row = &mut forged[0].items[1];
            match mutation {
                0 => row.declaration = NodeId(1),
                1 => row.generated.as_mut().unwrap().root = 1,
                2 => {
                    Arc::make_mut(&mut row.generated.as_mut().unwrap().tree)
                        .emitter
                        .event_path = vec![2]
                }
                _ => {
                    Arc::make_mut(&mut row.generated.as_mut().unwrap().tree).declarations[1] =
                        NodeId(4)
                }
            }
            assert!(
                load(&output, &forged).is_err(),
                "forged ledger mutation {mutation}"
            );
        }
        // This raw public reader proves structure only. Actual emission truth
        // and source scope authority are tested at Driver's immutable boundary.
    }
    #[test]
    fn generated_runtime_ct_requires_the_complete_frozen_deferred_body() {
        let source = SourceUnit::new(
            FileId(0),
            ModuleId(0),
            "source.zil",
            "struct Anchor<life a>{n:i32}#emit quote<ItemList>{};fn main()->i32{return 0}",
        );
        let parsed = zil_syntax::parse(&source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let paths = BTreeMap::new();
        let limits = Limits::default();
        let (rows, _) = capture_original(
            std::slice::from_ref(&source),
            &[&parsed.module],
            SourceIdentityContext::ClosedSnapshot {
                root: ModuleId(0),
                aliases: &paths,
                declarations: DeclarationPlacement::Original,
            },
            &limits,
        )
        .unwrap();
        let mut rows = rows.unwrap();
        let emitted = zil_syntax::parse(&SourceUnit::new(
            FileId(9),
            ModuleId(0),
            "generated.zil",
            "fn result<T:Copy,#N:i32>(marker:T)->i32{return #(N+1)}",
        ));
        assert!(emitted.diagnostics.is_empty(), "{:?}", emitted.diagnostics);
        let tree = Arc::new(GeneratedSourceTree {
            identity: SourceTreeIdentity::Generated {
                module: ModuleId(0),
                event_path: vec![0, 1],
            },
            emitter: SourceDeclarationSite {
                tree: SourceTreeIdentity::Original(ModuleId(0)),
                declaration: NodeId(1),
                event_path: vec![1],
            },
            module: Arc::new(emitted.module),
            declarations: vec![NodeId(4)],
        });
        let mut generated = zil_syntax::GeneratedSyntax::ItemList(tree.module.items.clone());
        crate::remap_generated_spans(&mut generated, parsed.module.items[1].span, &limits).unwrap();
        let zil_syntax::GeneratedSyntax::ItemList(mut generated) = generated else {
            unreachable!()
        };
        let mut output = parsed.module.clone();
        output.items[1] = generated.remove(0);
        rows[0].items[1] = DeclarationOrigin {
            closure_sites: crate::source_context::ClosureSiteLineage::Unavailable,
            declaration: NodeId(4),
            identity: tree.root_identity(0, &rows[0].items[0].identity).unwrap(),
            retained_members: None,
            generated: Some(GeneratedDeclarationOrigin {
                tree: tree.clone(),
                root: 0,
            }),
        };
        let load = |module: &Module, rows: &[crate::source_context::ModuleDeclarationOrigins]| {
            let mut budget = Budget::new(&limits);
            let context = SourceIdentityContext::ClosedSnapshot {
                root: ModuleId(0),
                aliases: &paths,
                declarations: DeclarationPlacement::Aligned(rows),
            }
            .resolve(std::slice::from_ref(&source), &mut budget)?;
            let mut origins = Origins::single(module, false, &mut budget)?;
            origins.capture_identities(module, false, &context, &mut budget)
        };
        load(&output, &rows).unwrap();
        let mut changed = output.clone();
        let ItemKind::Function(function) = &mut changed.items[1].kind else {
            unreachable!()
        };
        function.body = FunctionBody::Expression(Expr {
            span: function.span,
            kind: ExprKind::Literal(Literal::Integer("9".into())),
        });
        let error = load(&changed, &rows).unwrap_err();
        assert!(
            error.message.contains("complete frozen source body"),
            "{error:?}"
        );
        for mutation in 0..5 {
            let mut forged = rows.clone();
            let row = &mut forged[0].items[1];
            match mutation {
                0 => row.declaration = NodeId(1),
                1 => row.generated.as_mut().unwrap().root = 1,
                2 => {
                    Arc::make_mut(&mut row.generated.as_mut().unwrap().tree)
                        .emitter
                        .event_path = vec![2]
                }
                3 => Arc::make_mut(&mut row.generated.as_mut().unwrap().tree)
                    .declarations
                    .push(NodeId(4)),
                _ => row.generated = None,
            }
            assert!(
                load(&output, &forged).is_err(),
                "forged deferred runtime ledger {mutation}"
            );
        }
    }
}
