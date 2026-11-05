//! Frozen syntax sources are separate from output placement and permissions.
use std::sync::Arc;
use zil_base::{ModuleId, NodeId};
use zil_syntax::ast::Module;

/// Resolve an actual root/member structural path. Meta selection/unrolling is
/// deliberately not inferred: such events need their own source-tree ledger.
pub fn source_item_at_path<'a>(
    module: &'a Module,
    path: &[u32],
    max_depth: usize,
    mut pay: impl FnMut(usize) -> zil_base::CompileResult<()>,
) -> zil_base::CompileResult<Option<&'a zil_syntax::ast::Item>> {
    use zil_syntax::ast::ItemKind;
    pay(1)?;
    if path.len() > max_depth.saturating_add(1) {
        return Err(vec![zil_base::Diagnostic::limit(
            zil_base::Phase::Expand,
            "source item structural path exceeds depth budget",
            Some(module.span),
        )]);
    }
    let Some((root, members)) = path.split_first() else {
        return Ok(None);
    };
    let Some(mut item) = module.items.get(*root as usize) else {
        return Ok(None);
    };
    for ordinal in members {
        pay(1)?;
        let children = match &item.kind {
            ItemKind::Aggregate(host) => &host.methods,
            ItemKind::Trait(host) => &host.members,
            ItemKind::Impl(host) => &host.members,
            _ => return Ok(None),
        };
        let Some(child) = children.get(*ordinal as usize) else {
            return Ok(None);
        };
        item = child;
    }
    Ok(Some(item))
}

/// Structural source identity. No diagnostic span or hygienic name enters it.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum SourceTreeIdentity {
    Original(ModuleId),
    Generated {
        module: ModuleId,
        event_path: Vec<u32>,
    },
}
impl SourceTreeIdentity {
    pub fn module(&self) -> ModuleId {
        match self {
            Self::Original(module) | Self::Generated { module, .. } => *module,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceDeclarationSite {
    pub tree: SourceTreeIdentity,
    pub declaration: NodeId,
    /// Actual item/member structural path to the emission in that source tree.
    pub event_path: Vec<u32>,
}

/// One parse+hygiene result, frozen before diagnostic span remapping. The
/// inventory maps SourceIndex local declaration ordinals to distinct output
/// module IDs, including fields/members, never to the emitter's NodeId.
#[derive(Clone, Debug, PartialEq)]
pub struct GeneratedSourceTree {
    pub identity: SourceTreeIdentity,
    pub emitter: SourceDeclarationSite,
    pub module: Arc<Module>,
    pub declarations: Vec<NodeId>,
}
impl GeneratedSourceTree {
    pub fn identity_path_len(&self) -> usize {
        match &self.identity {
            SourceTreeIdentity::Original(_) => 1,
            SourceTreeIdentity::Generated { event_path, .. } => event_path.len().saturating_add(1),
        }
    }
    /// Canonical generated root binding, using only the actual event and root
    /// structure. The caller prepays the returned identity's owned strings.
    pub fn root_identity(
        &self,
        root: u32,
        namespace: &zil_ir::LifeDeclarationIdentity,
    ) -> Option<zil_ir::LifeDeclarationIdentity> {
        use zil_syntax::ast::{AggregateKind, ItemKind};
        let item = self.module.items.get(root as usize)?;
        let SourceTreeIdentity::Generated { event_path, .. } = &self.identity else {
            return None;
        };
        let kind = match &item.kind {
            ItemKind::Function(_) => "function",
            ItemKind::Extern { .. } => "extern",
            ItemKind::Aggregate(a) => match a.kind {
                AggregateKind::Struct => "struct",
                AggregateKind::Package => "package",
                AggregateKind::Resource => "resource",
            },
            ItemKind::Enum(_) => "enum",
            ItemKind::Trait(_) => "trait",
            ItemKind::Impl(_) => "impl",
            ItemKind::TypeAlias(_) => "alias",
            ItemKind::Const(_) => "const",
            ItemKind::Import(_) => "import",
            ItemKind::Meta(_) => "meta",
        };
        Some(zil_ir::LifeDeclarationIdentity {
            package: namespace.package.clone(),
            module_path: namespace.module_path.clone(),
            declaration_path: vec![kind.to_owned(), root.to_string()],
            generation_path: event_path.clone(),
        })
    }
    /// Address encoding only. A matching digest never authenticates the tree,
    /// emitter, declaration inventory, or any permission.
    pub fn generation_fingerprint(&self) -> Option<zil_base::Fingerprint> {
        let SourceTreeIdentity::Generated { event_path, .. } = &self.identity else {
            return None;
        };
        let mut hash = zil_base::FingerprintBuilder::new();
        hash.update(b"ZIL_GENERATED_SOURCE_TREE_V1");
        for part in event_path {
            hash.update(&part.to_le_bytes());
        }
        Some(hash.digest())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct GeneratedDeclarationOrigin {
    pub tree: Arc<GeneratedSourceTree>,
    /// Root ordinal in the frozen ItemList, independent of output placement.
    pub root: u32,
}

/// Request-local rows only address immutable trees. The row number is not a
/// semantic identity and confers no trust in the contained raw descriptors.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GeneratedSourceCatalog {
    pub trees: Vec<Arc<GeneratedSourceTree>>,
}
impl GeneratedSourceCatalog {
    pub fn get(&self, row: u32) -> Option<&Arc<GeneratedSourceTree>> {
        self.trees.get(row as usize)
    }
    pub fn find(&self, identity: &SourceTreeIdentity) -> Option<&Arc<GeneratedSourceTree>> {
        self.trees.iter().find(|tree| &tree.identity == identity)
    }
}
