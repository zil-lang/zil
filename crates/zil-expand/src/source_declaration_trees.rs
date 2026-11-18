//! Exact frozen declaration namespaces. Output positions never allocate IDs.
use super::*;
use crate::source_context::{GeneratedSourceTree, SourceTreeIdentity};
use std::collections::{BTreeMap, BTreeSet};

pub(super) struct Tree<'a> {
    pub(super) syntax: &'a s::Module,
    pub(super) generated: Option<&'a GeneratedSourceTree>,
    rows: BTreeMap<Vec<u32>, (NodeId, &'a s::Item)>,
    pub(super) count: u32,
}
impl<'a> Tree<'a> {
    fn global(&self, local: u32) -> CompileResult<NodeId> {
        self.generated.map_or(Ok(NodeId(local)), |tree| {
            tree.declarations
                .get(local as usize)
                .copied()
                .ok_or_else(|| {
                    failure(
                        self.syntax.span,
                        "generated namespace omits a source declaration",
                    )
                })
        })
    }
    fn reserve(&mut self, count: usize, budget: &mut Budget<'_>, span: Span) -> CompileResult<u32> {
        budget.work(count.saturating_add(1), span)?;
        let first = self.count;
        self.count = first
            .checked_add(
                u32::try_from(count)
                    .map_err(|_| failure(span, "source declaration count overflows"))?,
            )
            .ok_or_else(|| failure(span, "source declaration count overflows"))?;
        Ok(first)
    }
    fn row(
        &mut self,
        path: &[u32],
        local: u32,
        item: &'a s::Item,
        budget: &mut Budget<'_>,
    ) -> CompileResult<()> {
        budget.work(
            path.len()
                .saturating_mul(self.rows.len().checked_ilog2().unwrap_or(0) as usize + 2),
            item.span,
        )?;
        budget.bytes(path.len().saturating_mul(4).saturating_add(96), item.span)?;
        let id = self.global(local)?;
        if self.rows.insert(path.to_vec(), (id, item)).is_some() {
            return Err(failure(
                item.span,
                "source declaration has duplicate structural paths",
            ));
        }
        Ok(())
    }
    fn children(
        &mut self,
        item: &'a s::Item,
        path: &mut Vec<u32>,
        budget: &mut Budget<'_>,
    ) -> CompileResult<()> {
        if path.len() >= budget.limits.max_call_depth.min(128) {
            return Err(failure(
                item.span,
                "source declaration path exceeds its bounded depth",
            ));
        }
        let members = match &item.kind {
            ItemKind::Aggregate(host) => {
                self.reserve(host.fields.len(), budget, item.span)?;
                &host.methods
            }
            ItemKind::Trait(host) => &host.members,
            ItemKind::Impl(host) => &host.members,
            ItemKind::Enum(host) => {
                for variant in &host.variants {
                    let fields = match &variant.fields {
                        s::VariantFields::Unit => 0,
                        s::VariantFields::Tuple(fields) => fields.len(),
                        s::VariantFields::Record(fields) => fields.len(),
                    };
                    self.reserve(fields.saturating_add(1), budget, variant.span)?;
                }
                return Ok(());
            }
            _ => return Ok(()),
        };
        for (ordinal, member) in members.iter().enumerate() {
            let local = self.reserve(1, budget, member.span)?;
            let ordinal = u32::try_from(ordinal)
                .map_err(|_| failure(member.span, "source member ordinal overflows"))?;
            budget.push(path, ordinal, member.span)?;
            self.row(path, local, member, budget)?;
            self.children(member, path, budget)?;
            path.pop();
        }
        Ok(())
    }
    pub(super) fn new(
        syntax: &'a s::Module,
        generated: Option<&'a GeneratedSourceTree>,
        budget: &mut Budget<'_>,
    ) -> CompileResult<Self> {
        use crate::ct_selection::Visitor;
        for item in &syntax.items {
            budget.item(item, 0)?;
        }
        let mut tree = Self {
            syntax,
            generated,
            rows: BTreeMap::new(),
            count: 0,
        };
        tree.reserve(syntax.items.len(), budget, syntax.span)?;
        for (ordinal, item) in syntax.items.iter().enumerate() {
            let mut path = Vec::new();
            budget.push(&mut path, ordinal as u32, item.span)?;
            tree.row(&path, ordinal as u32, item, budget)?;
            tree.children(item, &mut path, budget)?;
        }
        if generated.is_some_and(|source| {
            source.declarations.len() != tree.count as usize
                || source
                    .declarations
                    .windows(2)
                    .any(|ids| ids[0].0.checked_add(1) != Some(ids[1].0))
        }) {
            return Err(failure(
                syntax.span,
                "generated namespace omits fields, variants or members",
            ));
        }
        Ok(tree)
    }
    pub(super) fn at(
        &self,
        path: &[u32],
        budget: &mut Budget<'_>,
        span: Span,
    ) -> CompileResult<(NodeId, &'a s::Item)> {
        budget.work(
            path.len()
                .saturating_mul(self.rows.len().checked_ilog2().unwrap_or(0) as usize + 2),
            span,
        )?;
        self.rows
            .get(path)
            .copied()
            .ok_or_else(|| failure(span, "actual declaration has no exact frozen source path"))
    }
}

pub(super) struct Namespace<'a> {
    pub(super) original: Tree<'a>,
    generated: Vec<Tree<'a>>,
    pub(super) next: u32,
}
impl<'a> Namespace<'a> {
    pub(super) fn new(
        origin: &'a ModuleDeclarationOrigins,
        budget: &mut Budget<'_>,
    ) -> CompileResult<Self> {
        let span = origin.original.span;
        if origin.original.id != origin.module {
            return Err(failure(span, "frozen original namespace changes module"));
        }
        let original = Tree::new(&origin.original, None, budget)?;
        let mut namespace = Self {
            next: original.count,
            original,
            generated: Vec::new(),
        };
        let mut ids = BTreeSet::new();
        for source in &origin.generated.trees {
            budget.work(
                namespace
                    .generated
                    .len()
                    .saturating_add(1)
                    .saturating_mul(source.identity_path_len()),
                span,
            )?;
            if namespace
                .generated
                .iter()
                .any(|old| old.generated.unwrap().identity == source.identity)
            {
                return Err(failure(
                    span,
                    "generated event has competing frozen source trees",
                ));
            }
            let SourceTreeIdentity::Generated { module, event_path } = &source.identity else {
                return Err(failure(span, "generated catalog contains an original tree"));
            };
            if *module != origin.module
                || source.module.id != origin.module
                || event_path.is_empty()
                || source.emitter.tree.module() != origin.module
                || source.emitter.event_path.is_empty()
            {
                return Err(failure(
                    span,
                    "generated event leaves its original module namespace",
                ));
            }
            let tree = Tree::new(&source.module, Some(source), budget)?;
            for id in &source.declarations {
                budget.work(ids.len().checked_ilog2().unwrap_or(0) as usize + 1, span)?;
                budget.bytes(48, span)?;
                if id.0 < namespace.original.count || !ids.insert(*id) {
                    return Err(failure(
                        span,
                        "source and generated declaration namespaces overlap",
                    ));
                }
                namespace.next = namespace.next.max(
                    id.0.checked_add(1)
                        .ok_or_else(|| failure(span, "generated namespace end overflows"))?,
                );
            }
            budget.push(&mut namespace.generated, tree, span)?;
        }
        // Catalog row order is an address convention, not source identity.
        // Every parent is resolved by its full event identity after indexing.
        for source in &origin.generated.trees {
            let SourceTreeIdentity::Generated { event_path, .. } = &source.identity else {
                unreachable!()
            };
            budget.work(
                namespace
                    .generated
                    .len()
                    .saturating_mul(source.identity_path_len()),
                span,
            )?;
            if source.is_selection() {
                budget.work(
                    event_path.len().saturating_add(source.emitter.event_path.len()),
                    span,
                )?;
                if !source.matches_selection_site() {
                    return Err(failure(span, "selected event changes its original selector path"));
                }
                let path = &source.emitter.event_path;
                let (id, mut authored) = namespace.original.at(&path[..1], budget, span)?;
                let mut has_iteration = false;
                for (step, event) in path[1..].chunks_exact(2).zip(event_path[2..].chunks_exact(3)) {
                    let items = match (&authored.kind, step[0]) {
                        (ItemKind::Meta(s::MetaItem::If { then_items, .. }), 1) => then_items,
                        (ItemKind::Meta(s::MetaItem::If { else_items, .. }), 0) => else_items,
                        (ItemKind::Meta(s::MetaItem::For { iterable, items, .. }), 2) => {
                            has_iteration = true;
                            let mut iterable = iterable;
                            while let s::ExprKind::Group(child) = &iterable.kind {
                                budget.work(1, span)?;
                                iterable = child;
                            }
                            if let s::ExprKind::Array(values) = &iterable.kind
                                && event[1] as usize >= values.len()
                            {
                                return Err(failure(span, "selected event has no original array iteration"));
                            }
                            items
                        }
                        _ => return Err(failure(span, "selected event does not name an original selector")),
                    };
                    authored = items.get(step[1] as usize).ok_or_else(|| {
                        failure(span, "selected event omits its original template declaration")
                    })?;
                }
                let selected = &source.module.items[0];
                let (authored_kind, authored_name) = crate::generics::origins::declaration_shape(authored);
                let (selected_kind, selected_name) = crate::generics::origins::declaration_shape(selected);
                budget.work(
                    authored_name.map_or(0, str::len).saturating_add(selected_name.map_or(0, str::len)),
                    span,
                )?;
                let same_name = authored_name == selected_name
                    || (!authored.public
                        && has_iteration
                        && selected_name.is_some_and(|name| {
                            name.starts_with("$ctfor222$")
                                && name.rsplit('$').next() == authored_name
                        }));
                if id != source.emitter.declaration
                    || authored_kind != selected_kind
                    || authored.public != selected.public
                    || !same_name
                {
                    return Err(failure(span, "selected event changes its original template declaration"));
                }
                continue;
            }
            let parent = match &source.emitter.tree {
                SourceTreeIdentity::Original(_) => {
                    if event_path.first() != Some(&0)
                        || event_path.get(1..) != Some(source.emitter.event_path.as_slice())
                    {
                        return Err(failure(
                            span,
                            "generated event changes its original emitter path",
                        ));
                    }
                    &namespace.original
                }
                SourceTreeIdentity::Generated {
                    event_path: parent, ..
                } => {
                    budget.work(parent.len().saturating_add(event_path.len()), span)?;
                    if !event_path.starts_with(parent)
                        || event_path.get(parent.len()) != Some(&1)
                        || event_path.get(parent.len() + 1..)
                            != Some(source.emitter.event_path.as_slice())
                    {
                        return Err(failure(
                            span,
                            "generated event changes its parent generation path",
                        ));
                    }
                    namespace
                        .generated
                        .iter()
                        .find(|row| row.generated.unwrap().identity == source.emitter.tree)
                        .ok_or_else(|| {
                            failure(span, "generated event omits its frozen emitter tree")
                        })?
                }
            };
            let (emitter, item) = parent.at(&source.emitter.event_path, budget, span)?;
            if emitter != source.emitter.declaration
                || !matches!(item.kind, ItemKind::Meta(s::MetaItem::Emit(_)))
            {
                return Err(failure(
                    span,
                    "generated event does not name its exact original emit declaration",
                ));
            }
        }
        Ok(namespace)
    }
    pub(super) fn root(
        &self,
        origin: &DeclarationOrigin,
        budget: &mut Budget<'_>,
        span: Span,
    ) -> CompileResult<(&Tree<'a>, u32)> {
        if let Some(generated) = &origin.generated {
            budget.work(
                self.generated
                    .len()
                    .saturating_mul(generated.tree.identity_path_len()),
                span,
            )?;
            let tree = self
                .generated
                .iter()
                .find(|tree| tree.generated.unwrap().identity == generated.tree.identity)
                .ok_or_else(|| {
                    failure(
                        span,
                        "actual generated root has no complete frozen event catalog",
                    )
                })?;
            // Catalog selection is structural; a same-address shortcut never
            // authenticates a different frozen tree carrying the same identity.
            let selected = tree.generated.unwrap();
            if !std::ptr::eq(selected, generated.tree.as_ref()) {
                use crate::ct_selection::Visitor;
                for item in &generated.tree.module.items {
                    budget.item(item, 0)?;
                }
                budget.work(
                    generated
                        .tree
                        .declarations
                        .len()
                        .saturating_add(generated.tree.identity_path_len()),
                    span,
                )?;
                if selected != generated.tree.as_ref() {
                    return Err(failure(
                        span,
                        "actual root substitutes another generated source tree",
                    ));
                }
            }
            if &origin.identity.generation_path
                != match &selected.identity {
                    SourceTreeIdentity::Generated { event_path, .. } => event_path,
                    _ => unreachable!(),
                }
            {
                return Err(failure(
                    span,
                    "actual root changes its full generation identity",
                ));
            }
            Ok((tree, generated.root))
        } else {
            if !origin.identity.generation_path.is_empty() {
                return Err(failure(
                    span,
                    "generated identity omits its frozen source event",
                ));
            }
            Ok((&self.original, origin.declaration.0))
        }
    }
}
